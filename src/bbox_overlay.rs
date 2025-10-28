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

            // Position label just above the bbox
            let label_position = Point::new(x, y - 16.0);

            // Draw text
            renderer.fill_text(
                Text {
                    content: annotation.category_name.clone(),
                    bounds: iced_core::Size::new(f32::INFINITY, 20.0),
                    size: 14.0.into(),
                    line_height: iced_core::text::LineHeight::default(),
                    font: renderer.default_font(),
                    horizontal_alignment: iced_core::alignment::Horizontal::Left,
                    vertical_alignment: iced_core::alignment::Vertical::Top,
                    shaping: iced_core::text::Shaping::Basic,
                    wrapping: iced_core::text::Wrapping::default(),
                },
                label_position,
                Color::from([1.0, 1.0, 0.0, 1.0]),
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
