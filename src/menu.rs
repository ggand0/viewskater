#[cfg(target_os = "linux")]
mod other_os {
    pub use iced;
    pub use iced_aw;
}

#[cfg(not(target_os = "linux"))]
mod macos {
    pub use iced_custom as iced;
    pub use iced_aw_custom as iced_aw;
}

#[cfg(target_os = "linux")]
use other_os::*;

#[cfg(not(target_os = "linux"))]
use macos::*;



use iced::widget::{
    row, button, text, svg,
};
use iced::alignment;
use iced::{Element, Length, Color, theme};
use iced_aw::menu::menu_tree::MenuTree;
use iced_aw::{helpers::menu_tree, menu_tree};
use crate::{Message, DataViewer};
use crate::toggler::toggler;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaneLayout {
    SinglePane,
    DualPane,
}

const MENU_FONT_SIZE : u16 = 16;
const MENU_ITEM_FONT_SIZE : u16 = 14;
const CARET_PATH : &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/svg/caret-right-fill.svg");


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

fn labeled_button <'a>(
    label: &str,
    text_size: u16,
    msg: Message
) -> button::Button<'a, Message, iced::Renderer> {
    button(text(label)
        .size(text_size)
        .width(Length::Fill)
        .height(Length::Fill)
        .vertical_alignment(alignment::Vertical::Center)
    )
    .padding([4, 8])
    .style(iced::theme::Button::Custom(Box::new(ButtonStyle {})))
    .on_press(msg)
}

fn nothing_button <'a>(label: &str, text_size: u16) -> button::Button<'a, Message, iced::Renderer> {
    labeled_button(label, text_size, Message::Nothing)
}

pub fn sub_menu_msg<'a>(
    label: &str,
    msg: Message,
    children: Vec<MenuTree<'a, Message, iced::Renderer>>,
) -> MenuTree<'a, Message, iced::Renderer> {
    let handle = svg::Handle::from_path(CARET_PATH);
    let arrow = svg(handle)
        .width(Length::Shrink)
        .style(theme::Svg::custom_fn(|theme| svg::Appearance {
            color: Some(theme.extended_palette().background.base.text),
        }));

    menu_tree(
        base_button(
            row![
                text(label)
                .size(MENU_ITEM_FONT_SIZE)
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


fn build_menu_items_v1<'a>() -> Vec<MenuTree<'a, Message, iced::Renderer>> {
    let menu_items = vec![
        labeled_button(&String::from("Open Folder (Alt+1 or 2)"), MENU_ITEM_FONT_SIZE, Message::OpenFolder(0) ),
        labeled_button(&String::from("Open File (Alt+Ctrl+1 or 2)"), MENU_ITEM_FONT_SIZE, Message::OpenFile(0) ),
        labeled_button(&String::from("Close (Ctrl+W)"), MENU_ITEM_FONT_SIZE, Message::Close ),
        labeled_button(&String::from("Quit (Ctrl+Q)"), MENU_ITEM_FONT_SIZE, Message::Quit ),
    ];
    menu_items.into_iter().map(|item| menu_tree!(item.width(Length::Fill).height(Length::Fill))).collect()
}

pub fn menu_3<'a>(app: &DataViewer) -> MenuTree<'a, Message, iced::Renderer> {
    // Other menu items...

    // Create a submenu for pane layout selection
    let pane_layout_submenu = sub_menu_msg(
        "Pane Layout",
        Message::Nothing,
        vec![
            menu_tree!(
                labeled_button("Single Pane (Ctrl+1)", MENU_ITEM_FONT_SIZE, Message::TogglePaneLayout(PaneLayout::SinglePane))
                .width(Length::Fill)
            ),
            menu_tree!(
                labeled_button("Dual Pane (Ctrl+2)", MENU_ITEM_FONT_SIZE, Message::TogglePaneLayout(PaneLayout::DualPane))
                .width(Length::Fill)
            ),

        ],
    );

    let root = menu_tree(
        nothing_button("Controls", MENU_FONT_SIZE),
        vec![
            // Other menu items...
            // separator(),
            pane_layout_submenu,
            menu_tree!(row![
                toggler::Toggler::new(
                    Some("Toggle Slider (Space)".into()),
                    app.is_slider_dual,
                    Message::ToggleSliderType,
                )
            ].padding([4, 8])),
            menu_tree!(row!(
                toggler::Toggler::new(
                    Some("Toggle Footer (Tab)".into()),
                    app.show_footer,
                    Message::ToggleFooter,
                )
            ).padding([4, 8]))
        ],
    );

    root
}

pub fn menu_1<'a>(_app: &DataViewer) -> MenuTree<'a, Message, iced::Renderer> {
    let c = build_menu_items_v1();
    let root = menu_tree(
        nothing_button("File", MENU_FONT_SIZE),
        c
    );

    root
}
