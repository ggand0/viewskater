/// Bounding box overlay rendering for COCO annotations
///
/// This module renders bounding boxes using a custom WGPU shader.
use iced_winit::core::{Element, Length, Color, Rectangle, Point, Vector};
use iced_winit::core::Theme as WinitTheme;
use iced_wgpu::Renderer;
use iced_widget::{Stack, container, text, column};
use iced_core::Border;

use crate::app::Message;
use crate::coco::parser::{ImageAnnotation, CocoSegmentation};
use crate::settings::CocoMaskRenderMode;
use super::bbox_shader::BBoxShader;
use super::polygon_shader::PolygonShader;
use super::mask_shader::MaskShader;

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

/// Render bounding box and segmentation mask overlays for a list of annotations
///
/// Uses custom WGPU shader for rendering actual bbox rectangles with text labels.
/// Renders segmentation masks as semi-transparent filled polygons or pixel-perfect textures.
/// Applies zoom transformation based on scale and offset parameters.
pub fn render_bbox_overlay<'a>(
    annotations: &'a [ImageAnnotation],
    image_size: (u32, u32),
    zoom_scale: f32,
    zoom_offset: Vector,
    show_bboxes: bool,
    show_masks: bool,
    has_invalid_annotations: bool,
    render_mode: CocoMaskRenderMode,
    disable_simplification: bool,
) -> Element<'a, Message, WinitTheme, Renderer> {
    if annotations.is_empty() {
        return container(iced_widget::Space::new(Length::Fill, Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    }

    // Stack for layering visualizations
    let mut stack = Stack::new();

    // Segmentation masks (rendered first, behind bboxes)
    if show_masks {
        let mask_element: Element<'a, Message, WinitTheme, Renderer> = match render_mode {
            CocoMaskRenderMode::Polygon => {
                // Polygon-based rendering (vector, scalable)
                PolygonShader::new(annotations.to_vec(), image_size, zoom_scale, zoom_offset, disable_simplification)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            }
            CocoMaskRenderMode::Pixel => {
                // Pixel-based rendering (raster, exact)
                MaskShader::new(annotations.to_vec(), image_size, zoom_scale, zoom_offset)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            }
        };
        stack = stack.push(mask_element);
    }

    // Bbox rectangles
    if show_bboxes {
        let bbox_shader = BBoxShader::new(annotations.to_vec(), image_size, zoom_scale, zoom_offset)
            .width(Length::Fill)
            .height(Length::Fill);
        stack = stack.push(bbox_shader);

        // Create per-bbox label overlay
        let labels_overlay = BBoxLabels::into_element(annotations.to_vec(), image_size, zoom_scale, zoom_offset);
        stack = stack.push(labels_overlay);
    }

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

    // Add invalid annotation warning at the top if needed
    if has_invalid_annotations {
        summary = summary.push(
            text("WARNING: Invalid annotations skipped")
                .size(14)
                .style(|_theme| iced_widget::text::Style {
                    color: Some(Color::from([1.0, 0.5, 0.0, 1.0]))  // Orange warning
                })
        );
    }

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

    // Add category summary on top
    stack = stack.push(summary_container);

    stack.into()
}

/// Widget for rendering per-bbox labels
struct BBoxLabels {
    annotations: Vec<ImageAnnotation>,
    image_size: (u32, u32),
    zoom_scale: f32,
    zoom_offset: Vector,
}

impl BBoxLabels {
    fn into_element(annotations: Vec<ImageAnnotation>, image_size: (u32, u32), zoom_scale: f32, zoom_offset: Vector) -> Element<'static, Message, WinitTheme, Renderer> {
        let widget = Self {
            annotations,
            image_size,
            zoom_scale,
            zoom_offset,
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

        // Base scale from ContentFit::Contain
        let width_ratio = display_width / image_width;
        let height_ratio = display_height / image_height;
        let base_scale = width_ratio.min(height_ratio);

        // Calculate zoomed image dimensions (changes with zoom)
        let zoomed_image_width = image_width * base_scale * self.zoom_scale;
        let zoomed_image_height = image_height * base_scale * self.zoom_scale;

        // Centering offset after zoom (changes as image grows/shrinks)
        let center_offset_x = (display_width - zoomed_image_width) / 2.0;
        let center_offset_y = (display_height - zoomed_image_height) / 2.0;

        // Draw label for each bbox
        for annotation in &self.annotations {
            // Scale bbox coordinates by base_scale and zoom_scale
            let scaled_bbox_x = annotation.bbox.x * base_scale * self.zoom_scale;
            let scaled_bbox_y = annotation.bbox.y * base_scale * self.zoom_scale;

            // Apply centering offset and pan offset (subtract offset like ImageShader does)
            let x = scaled_bbox_x + center_offset_x - self.zoom_offset.x + bounds.x;
            let y = scaled_bbox_y + center_offset_y - self.zoom_offset.y + bounds.y;

            // Get color for this category
            let bg_color = get_category_color(annotation.category_id);

            // Estimate text width (rough approximation) and scale with zoom
            let base_text_width = annotation.category_name.len() as f32 * 7.5;
            let text_width = base_text_width * self.zoom_scale;
            let base_label_height = 18.0;
            let label_height = base_label_height * self.zoom_scale;
            let padding = 4.0 * self.zoom_scale;

            // Position label just above the bbox
            let label_y = y - label_height - 2.0 * self.zoom_scale;

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
                    size: (13.0 * self.zoom_scale).into(),
                    line_height: iced_core::text::LineHeight::default(),
                    font: renderer.default_font(),
                    horizontal_alignment: iced_core::alignment::Horizontal::Left,
                    vertical_alignment: iced_core::alignment::Vertical::Top,
                    shaping: iced_core::text::Shaping::Basic,
                    wrapping: iced_core::text::Wrapping::default(),
                },
                Point::new(x + padding, label_y + 2.0 * self.zoom_scale),
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

/// Widget for rendering segmentation masks
struct SegmentationMasks {
    annotations: Vec<ImageAnnotation>,
    image_size: (u32, u32),
    zoom_scale: f32,
    zoom_offset: Vector,
}

impl SegmentationMasks {
    #[allow(dead_code, clippy::new_ret_no_self)]
    fn new(annotations: Vec<ImageAnnotation>, image_size: (u32, u32), zoom_scale: f32, zoom_offset: Vector) -> Element<'static, Message, WinitTheme, Renderer> {
        let widget = Self {
            annotations,
            image_size,
            zoom_scale,
            zoom_offset,
        };
        Element::new(widget)
    }
}

impl<Theme, R> iced_core::Widget<Message, Theme, R> for SegmentationMasks
where
    R: iced_core::Renderer,
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
        let bounds = layout.bounds();

        // Calculate scaling (same as BBoxShader)
        let image_width = self.image_size.0 as f32;
        let image_height = self.image_size.1 as f32;
        let display_width = bounds.width;
        let display_height = bounds.height;

        // Base scale from ContentFit::Contain
        let width_ratio = display_width / image_width;
        let height_ratio = display_height / image_height;
        let base_scale = width_ratio.min(height_ratio);

        // Calculate zoomed image dimensions
        let zoomed_image_width = image_width * base_scale * self.zoom_scale;
        let zoomed_image_height = image_height * base_scale * self.zoom_scale;

        // Centering offset after zoom
        let center_offset_x = (display_width - zoomed_image_width) / 2.0;
        let center_offset_y = (display_height - zoomed_image_height) / 2.0;

        // Draw masks for each annotation
        for annotation in &self.annotations {
            if let Some(ref segmentation) = annotation.segmentation {
                let color = get_category_color(annotation.category_id);
                let mask_color = Color::from_rgba(color.r, color.g, color.b, 0.4); // 40% opacity

                match segmentation {
                    CocoSegmentation::Polygon(polygons) => {
                        // Render each polygon as a filled shape
                        for polygon in polygons {
                            self.draw_polygon(
                                renderer,
                                polygon,
                                mask_color,
                                bounds,
                                base_scale,
                                center_offset_x,
                                center_offset_y,
                            );
                        }
                    }
                    CocoSegmentation::Rle(_rle) => {
                        // RLE rendering not yet implemented
                        // Could decode RLE to polygon or render as pixel mask
                    }
                }
            }
        }
    }
}

impl SegmentationMasks {
    #[allow(clippy::too_many_arguments)]
    fn draw_polygon<R: iced_core::Renderer>(
        &self,
        renderer: &mut R,
        polygon: &[f32],
        color: Color,
        bounds: Rectangle,
        base_scale: f32,
        center_offset_x: f32,
        center_offset_y: f32,
    ) {
        // Polygon format: [x1, y1, x2, y2, x3, y3, ...]
        if polygon.len() < 6 {
            return; // Need at least 3 points (6 coordinates)
        }

        // Transform polygon vertices to screen coordinates
        let mut points = Vec::new();
        for i in (0..polygon.len()).step_by(2) {
            if i + 1 >= polygon.len() {
                break;
            }

            let x = polygon[i];
            let y = polygon[i + 1];

            // Apply same transformation as bboxes
            let scaled_x = x * base_scale * self.zoom_scale;
            let scaled_y = y * base_scale * self.zoom_scale;

            let screen_x = scaled_x + center_offset_x - self.zoom_offset.x + bounds.x;
            let screen_y = scaled_y + center_offset_y - self.zoom_offset.y + bounds.y;

            points.push(Point::new(screen_x, screen_y));
        }

        // Draw filled polygon using quad filling (approximate with triangles)
        if points.len() >= 3 {
            // Simple triangle fan from first point
            for i in 1..points.len() - 1 {
                self.draw_triangle(renderer, points[0], points[i], points[i + 1], color);
            }
        }
    }

    fn draw_triangle<R: iced_core::Renderer>(
        &self,
        renderer: &mut R,
        p1: Point,
        p2: Point,
        p3: Point,
        color: Color,
    ) {
        // Calculate bounding box for the triangle
        let min_x = p1.x.min(p2.x).min(p3.x);
        let max_x = p1.x.max(p2.x).max(p3.x);
        let min_y = p1.y.min(p2.y).min(p3.y);
        let max_y = p1.y.max(p2.y).max(p3.y);

        // Fill the triangle using a filled quad (approximation)
        renderer.fill_quad(
            iced_core::renderer::Quad {
                bounds: Rectangle {
                    x: min_x,
                    y: min_y,
                    width: max_x - min_x,
                    height: max_y - min_y,
                },
                border: Border::default(),
                shadow: iced_core::Shadow::default(),
            },
            color,
        );
    }
}

impl<'a, Theme, R> From<SegmentationMasks> for Element<'a, Message, Theme, R>
where
    R: iced_core::Renderer + 'a,
{
    fn from(widget: SegmentationMasks) -> Self {
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
