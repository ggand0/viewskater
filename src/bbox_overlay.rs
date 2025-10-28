/// Bounding box overlay rendering for COCO annotations
///
/// This module renders bounding boxes using a custom WGPU shader.

use iced_winit::core::{Element, Length, Color};
use iced_winit::core::Theme as WinitTheme;
use iced_wgpu::Renderer;
use iced_widget::{Stack, container, text, column};

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

    // Text labels overlay
    let mut labels = column![];

    for annotation in annotations.iter().take(10) {
        let label_text = format!("{} [{:.0},{:.0}] {:.0}x{:.0}",
            annotation.category_name,
            annotation.bbox.x,
            annotation.bbox.y,
            annotation.bbox.width,
            annotation.bbox.height
        );

        labels = labels.push(
            text(label_text)
                .size(14)
                .style(|_theme| iced_widget::text::Style {
                    color: Some(Color::from([1.0, 1.0, 0.0, 1.0]))
                })
        );
    }

    if annotations.len() > 10 {
        labels = labels.push(
            text(format!("... {} more", annotations.len() - 10))
                .size(12)
                .style(|_theme| iced_widget::text::Style {
                    color: Some(Color::from([1.0, 1.0, 0.0, 0.7]))
                })
        );
    }

    let labels_container = container(labels)
        .padding(8)
        .style(|_theme: &WinitTheme| iced_widget::container::Style {
            background: Some(Color::from([0.0, 0.0, 0.0, 0.7]).into()),
            border: iced_winit::core::Border {
                radius: 4.0.into(),
                width: 1.0,
                color: Color::from([1.0, 1.0, 0.0, 0.8]),
            },
            ..iced_widget::container::Style::default()
        });

    // Stack bbox rectangles and labels
    Stack::new()
        .push(bbox_shader)
        .push(labels_container)
        .into()
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
