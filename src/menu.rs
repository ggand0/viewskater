/*#[cfg(target_os = "macos")]
mod macos {
    pub use iced_custom as iced;
    pub use iced_aw_custom as iced_aw;
    pub use iced_widget_custom as iced_widget;
}

#[cfg(not(target_os = "macos"))]
mod other_os {
    pub use iced;
    pub use iced_aw;
    pub use iced_widget;
}

#[cfg(target_os = "macos")]
use macos::*;

#[cfg(not(target_os = "macos"))]
use other_os::*;*/

#[cfg(target_os = "linux")]
mod other_os {
    pub use iced;
    pub use iced_aw;
    pub use iced_widget;
}

#[cfg(not(target_os = "linux"))]
mod macos {
    pub use iced_custom as iced;
    pub use iced_aw_custom as iced_aw;
    pub use iced_widget_custom as iced_widget;
}

#[cfg(target_os = "linux")]
use other_os::*;

#[cfg(not(target_os = "linux"))]
use macos::*;
// Continue with your code using the imported modules



use iced::widget::{
    row, button, text, svg, toggler
};
use iced::alignment;
use iced::{Element, Length, Color, theme};

use iced_aw::menu::menu_tree::MenuTree;
use iced_aw::{helpers::menu_tree, menu_tree};

use crate::{Message, DataViewer};

// use iced::widget::container;
//use iced::Theme;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaneLayout {
    SinglePane,
    DualPane,
}


struct ButtonStyle;
impl button::StyleSheet for ButtonStyle {
    type Style = iced::Theme;

    fn active(&self, style: &Self::Style) -> button::Appearance {
        button::Appearance {
            text_color: style.extended_palette().background.base.text,
            border_radius: [4.0; 4].into(),
            background: Some(Color::TRANSPARENT.into()),
            ..Default::default()
        }
    }

    fn hovered(&self, style: &Self::Style) -> button::Appearance {
        let plt = style.extended_palette();

        button::Appearance {
            background: Some(plt.primary.weak.color.into()),
            text_color: plt.primary.weak.text,
            ..self.active(style)
        }
    }
}

fn base_button<'a>(
    content: impl Into<Element<'a, Message, iced::Renderer>>,
    msg: Message,
) -> button::Button<'a, Message, iced::Renderer> {
    button(content)
        .padding([4, 8])
        .style(iced::theme::Button::Custom(Box::new(ButtonStyle {})))
        .on_press(msg)
}

fn labeled_button<'a>(label: &str, msg: Message) -> button::Button<'a, Message, iced::Renderer> {
    base_button(
        text(label)
            .width(Length::Fill)
            .height(Length::Fill)
            .vertical_alignment(alignment::Vertical::Center),
        msg,
    )
}

/*fn debug_button<'a>(label: &str) -> button::Button<'a, Message, iced::Renderer> {
    labeled_button(label, Message::Debug(label.into()))
}*/
fn nothing_button<'a>(label: &str) -> button::Button<'a, Message, iced::Renderer> {
    labeled_button(label, Message::Nothing)
}

pub fn sub_menu_msg<'a>(
    label: &str,
    msg: Message,
    children: Vec<MenuTree<'a, Message, iced::Renderer>>,
) -> MenuTree<'a, Message, iced::Renderer> {
    let handle = svg::Handle::from_path(format!(
        "{}/caret-right-fill.svg",
        env!("CARGO_MANIFEST_DIR")
    ));
    let arrow = svg(handle)
        .width(Length::Shrink)
        .style(theme::Svg::custom_fn(|theme| svg::Appearance {
            color: Some(theme.extended_palette().background.base.text),
        }));

    menu_tree(
        base_button(
            row![
                text(label)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .vertical_alignment(alignment::Vertical::Center),
                arrow
            ]
            .align_items(iced::Alignment::Center),
            msg,
        )
        .width(Length::Fill)
        .height(Length::Fill),
        children,
    )
}

/*fn base_label<'a>(
    content: impl Into<Element<'a, Message, iced::Renderer>>,
) -> button::Button<'a, Message, iced::Renderer> {
    button(content)
        .padding([4, 8])
        .style(iced::theme::Button::Custom(Box::new(ButtonStyle {})))
}

fn sub_menu<'a>(
    label: &str,
    children: Vec<MenuTree<'a, Message, iced::Renderer>>,
) -> MenuTree<'a, Message, iced::Renderer> {
    let handle = svg::Handle::from_path(format!(
        "{}/caret-right-fill.svg",
        env!("CARGO_MANIFEST_DIR")
    ));
    let arrow = svg(handle)
        .width(Length::Shrink)
        .style(theme::Svg::custom_fn(|theme| svg::Appearance {
            color: Some(theme.extended_palette().background.base.text),
        }));

    menu_tree(
        base_label(
            row![
                text(label)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .vertical_alignment(alignment::Vertical::Center),
                arrow
            ]
            .align_items(iced::Alignment::Center),
            // No specific message passed here
        )
        .width(Length::Fill)
        .height(Length::Fill),
        children,
    )
}*/


fn build_menu_items_v1<'a>() -> Vec<MenuTree<'a, Message, iced::Renderer>> {
    let menu_items = vec![
        // labeled_button(label, Message::OpenFolder(label.into()))
        labeled_button(&String::from("Open Folder"), Message::OpenFolder(0) ),
        labeled_button(&String::from("Open File"), Message::OpenFile(0) ),
        labeled_button(&String::from("Close"), Message::Close ),
        
    ];
    menu_items.into_iter().map(|item| menu_tree!(item.width(Length::Fill).height(Length::Fill))).collect()
}


pub fn menu_3<'a>(app: &DataViewer) -> MenuTree<'a, Message, iced::Renderer> {
    // Other menu items...

    // Create a submenu for pane layout selection
    let pane_layout_submenu = sub_menu_msg(
        "Pane Layout",
        // Message::Debug(String::from("Pane Layout")),
        Message::Nothing,
        vec![
            menu_tree!(
                labeled_button("Single Pane", Message::TogglePaneLayout(PaneLayout::SinglePane))
            ),
            menu_tree!(
                labeled_button("Dual Pane", Message::TogglePaneLayout(PaneLayout::DualPane))
            ),

        ],
    );

    let root = menu_tree(
        nothing_button("Controls"),
        vec![
            // Other menu items...
            // separator(),
            pane_layout_submenu,
            menu_tree!(row![toggler(
                Some("Toggle Slider".into()),
                app.is_slider_dual,
                Message::ToggleSliderType
            )]),
            menu_tree!(row!(toggler(
                Some("Toggle Footer".into()),
                app.show_footer,
                Message::ToggleFooter
            )))
        ],
    );

    root
}

pub fn menu_1<'a>(_app: &DataViewer) -> MenuTree<'a, Message, iced::Renderer> {
    let c = build_menu_items_v1();
    let root = menu_tree(
        nothing_button("File"),
        c
    )
    .width(110);

    root
}

// 022524: This one doesn't seem to be used
/*mod style {
    #[cfg(not(target_os = "linux"))]
    use iced_custom as iced;

    use iced::widget::container;
    use iced::Theme;

    pub fn title_bar_active(theme: &Theme) -> container::Appearance {
        let palette = theme.extended_palette();

        container::Appearance {
            text_color: Some(palette.background.strong.text),
            background: Some(palette.background.strong.color.into()),
            ..Default::default()
        }
    }

    pub fn title_bar_focused(theme: &Theme) -> container::Appearance {
        let palette = theme.extended_palette();

        container::Appearance {
            text_color: Some(palette.primary.strong.text),
            background: Some(palette.primary.strong.color.into()),
            ..Default::default()
        }
    }

    pub fn pane_active(theme: &Theme) -> container::Appearance {
        let palette = theme.extended_palette();

        container::Appearance {
            background: Some(palette.background.weak.color.into()),
            border_width: 2.0,
            border_color: palette.background.strong.color,
            ..Default::default()
        }
    }

    pub fn pane_focused(theme: &Theme) -> container::Appearance {
        let palette = theme.extended_palette();

        container::Appearance {
            background: Some(palette.background.weak.color.into()),
            border_width: 2.0,
            border_color: palette.primary.strong.color,
            ..Default::default()
        }
    }
}*/
