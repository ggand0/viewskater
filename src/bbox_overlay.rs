/// Bounding box overlay rendering for COCO annotations
///
/// This module renders bounding boxes using a custom WGPU shader.

use iced_winit::core::{Element, Length, Color, Rectangle, Point};
use iced_winit::core::Theme as WinitTheme;
use iced_wgpu::Renderer;
use iced_widget::{Stack, container, text, column};
use iced_core::Border;

use crate::app::Message;
use crate::coco_parser::ImageAnnotation;
use crate::widgets::shader::bbox_shader::BBoxShader;

/// Get YOLO color for category ID (same as bbox_shader)
fn get_category_color(category_id: u64) -> Color {
    let colors = [
        [0.000, 0.447, 0.741], [0.850, 0.325, 0.098], [0.929, 0.694, 0.125],
        [0.494, 0.184, 0.556], [0.466, 0.674, 0.188], [0.301, 0.745, 0.933],
        [0.635, 0.078, 0.184], [0.300, 0.300, 0.300], [0.600, 0.600, 0.600],
        [1.000, 0.000, 0.000], [1.000, 0.500, 0.000], [0.749, 0.749, 0.000],
        [0.000, 1.000, 0.000], [0.000, 0.000, 1.000], [0.667, 0.000, 1.000],
        [0.333, 0.333, 0.000], [0.333, 0.667, 0.000], [0.333, 1.000, 0.000],
        [0.667, 0.333, 0.000], [0.667, 0.667, 0.000], [0.667, 1.000, 0.000],
        [1.000, 0.333, 0.000], [1.000, 0.667, 0.000], [1.000, 1.000, 0.000],
        [0.000, 0.333, 0.500], [0.000, 0.667, 0.500], [0.000, 1.000, 0.500],
        [0.333, 0.000, 0.500], [0.333, 0.333, 0.500], [0.333, 0.667, 0.500],
        [0.333, 1.000, 0.500], [0.667, 0.000, 0.500], [0.667, 0.333, 0.500],
        [0.667, 0.667, 0.500], [0.667, 1.000, 0.500], [1.000, 0.000, 0.500],
        [1.000, 0.333, 0.500], [1.000, 0.667, 0.500], [1.000, 1.000, 0.500],
        [0.000, 0.333, 1.000], [0.000, 0.667, 1.000], [0.000, 1.000, 1.000],
        [0.333, 0.000, 1.000], [0.333, 0.333, 1.000], [0.333, 0.667, 1.000],
        [0.333, 1.000, 1.000], [0.667, 0.000, 1.000], [0.667, 0.333, 1.000],
        [0.667, 0.667, 1.000], [0.667, 1.000, 1.000], [1.000, 0.000, 1.000],
        [1.000, 0.333, 1.000], [1.000, 0.667, 1.000], [0.333, 0.000, 0.000],
        [0.500, 0.000, 0.000], [0.667, 0.000, 0.000], [0.833, 0.000, 0.000],
        [1.000, 0.000, 0.000], [0.000, 0.167, 0.000], [0.000, 0.333, 0.000],
        [0.000, 0.500, 0.000], [0.000, 0.667, 0.000], [0.000, 0.833, 0.000],
        [0.000, 1.000, 0.000], [0.000, 0.000, 0.167], [0.000, 0.000, 0.333],
        [0.000, 0.000, 0.500], [0.000, 0.000, 0.667], [0.000, 0.000, 0.833],
        [0.000, 0.000, 1.000], [0.000, 0.000, 0.000], [0.143, 0.143, 0.143],
        [0.286, 0.286, 0.286], [0.429, 0.429, 0.429], [0.571, 0.571, 0.571],
        [0.714, 0.714, 0.714], [0.857, 0.857, 0.857], [0.000, 0.447, 0.741],
        [0.314, 0.717, 0.741], [0.500, 0.500, 0.000],
    ];
    let idx = (category_id - 1) as usize % colors.len();
    let rgb = colors[idx];
    Color::from_rgb(rgb[0], rgb[1], rgb[2])
}

/// Render bounding box overlays for a list of annotations
///
/// Uses custom WGPU shader for rendering actual bbox rectangles with text labels.
pub fn render_bbox_overlay<'a>(
    annotations: &'a [ImageAnnotation],
    image_size: (u32, u32),
) -> Element<'a, Message, WinitTheme, Renderer> {
    if annotations.is_empty() {
        return container(iced_widget::Space::new(Length::Fill, Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    }

    // Bbox rectangles
    let bbox_shader = BBoxShader::new(annotations.to_vec(), image_size)
        .width(Length::Fill)
        .height(Length::Fill);

    // Create per-bbox label overlay
    let labels_overlay = BBoxLabels::new(annotations.to_vec(), image_size);

    // Category summary: count occurrences of each category
    let mut category_counts = std::collections::HashMap::new();
    for annotation in annotations {
        *category_counts.entry(annotation.category_name.as_str()).or_insert(0) += 1;
    }

    // Sort by count descending, then by category name for stable ordering
    let mut sorted_categories: Vec<_> = category_counts.into_iter().collect();
    sorted_categories.sort_by(|a, b| {
        b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0))  // Primary: count desc, Secondary: name asc
    });

    // Build category summary text
    let mut summary = column![];
    for (category, count) in sorted_categories {
        let label_text = format!("{} {}", count, category);
        summary = summary.push(
            text(label_text)
                .size(14)
                .style(|_theme| iced_widget::text::Style {
                    color: Some(Color::from([1.0, 1.0, 0.0, 1.0]))
                })
        );
    }

    let summary_container = container(summary)
        .padding(8)
        .style(|_theme: &WinitTheme| iced_widget::container::Style {
            background: Some(Color::from([0.0, 0.0, 0.0, 0.7]).into()),
            border: Border {
                radius: 4.0.into(),
                width: 1.0,
                color: Color::from([1.0, 1.0, 0.0, 0.8]),
            },
            ..iced_widget::container::Style::default()
        });

    // Stack bbox rectangles, per-bbox labels, and category summary
    Stack::new()
        .push(bbox_shader)
        .push(labels_overlay)
        .push(summary_container)
        .into()
}

/// Widget for rendering per-bbox labels
struct BBoxLabels {
    annotations: Vec<ImageAnnotation>,
    image_size: (u32, u32),
}

impl BBoxLabels {
    fn new(annotations: Vec<ImageAnnotation>, image_size: (u32, u32)) -> Element<'static, Message, WinitTheme, Renderer> {
        let widget = Self {
            annotations,
            image_size,
        };
        Element::new(widget)
    }
}

impl<Theme, R> iced_core::Widget<Message, Theme, R> for BBoxLabels
where
    R: iced_core::Renderer + iced_core::text::Renderer,
{
    fn size(&self) -> iced_core::Size<Length> {
        iced_core::Size {
            width: Length::Fill,
            height: Length::Fill,
        }
    }

    fn layout(
        &self,
        _tree: &mut iced_core::widget::Tree,
        _renderer: &R,
        limits: &iced_core::layout::Limits,
    ) -> iced_core::layout::Node {
        iced_core::layout::atomic(limits, Length::Fill, Length::Fill)
    }

    fn draw(
        &self,
        _tree: &iced_core::widget::Tree,
        renderer: &mut R,
        _theme: &Theme,
        _style: &iced_core::renderer::Style,
        layout: iced_core::layout::Layout<'_>,
        _cursor: iced_core::mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        use iced_core::text::Text;

        let bounds = layout.bounds();

        // Calculate scaling (same as BBoxShader)
        let image_width = self.image_size.0 as f32;
        let image_height = self.image_size.1 as f32;
        let display_width = bounds.width;
        let display_height = bounds.height;

        let width_ratio = display_width / image_width;
        let height_ratio = display_height / image_height;
        let scale = width_ratio.min(height_ratio);

        let scaled_width = image_width * scale;
        let scaled_height = image_height * scale;
        let offset_x = (display_width - scaled_width) / 2.0;
        let offset_y = (display_height - scaled_height) / 2.0;

        // Draw label for each bbox
        for annotation in &self.annotations {
            let x = annotation.bbox.x * scale + offset_x + bounds.x;
            let y = annotation.bbox.y * scale + offset_y + bounds.y;

            // Get color for this category
            let bg_color = get_category_color(annotation.category_id);

            // Estimate text width (rough approximation)
            let text_width = annotation.category_name.len() as f32 * 7.5;
            let label_height = 18.0;
            let padding = 4.0;

            // Position label just above the bbox
            let label_y = y - label_height - 2.0;

            // Draw colored background rectangle
            renderer.fill_quad(
                iced_core::renderer::Quad {
                    bounds: Rectangle {
                        x,
                        y: label_y,
                        width: text_width + padding * 2.0,
                        height: label_height,
                    },
                    border: Border {
                        radius: 2.0.into(),
                        width: 0.0,
                        color: Color::TRANSPARENT,
                    },
                    shadow: iced_core::Shadow::default(),
                },
                bg_color,
            );

            // Draw white text on colored background
            renderer.fill_text(
                Text {
                    content: annotation.category_name.clone(),
                    bounds: iced_core::Size::new(f32::INFINITY, label_height),
                    size: 13.0.into(),
                    line_height: iced_core::text::LineHeight::default(),
                    font: renderer.default_font(),
                    horizontal_alignment: iced_core::alignment::Horizontal::Left,
                    vertical_alignment: iced_core::alignment::Vertical::Top,
                    shaping: iced_core::text::Shaping::Basic,
                    wrapping: iced_core::text::Wrapping::default(),
                },
                Point::new(x + padding, label_y + 2.0),
                Color::WHITE,
                bounds,
            );
        }
    }
}

impl<'a, Theme, R> From<BBoxLabels> for Element<'a, Message, Theme, R>
where
    R: iced_core::Renderer + iced_core::text::Renderer + 'a,
{
    fn from(widget: BBoxLabels) -> Self {
        Element::new(widget)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_category_colors() {
        // Test that we get different colors for different indices
        let color0 = get_category_color(0);
        let color1 = get_category_color(1);
        assert_ne!(color0, color1);

        // Test wrapping
        let color10 = get_category_color(10);
        let color0_again = get_category_color(0);
        assert_eq!(color10, color0_again);
    }
}
