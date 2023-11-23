use iced::widget::{
    row, button, text, svg,
};
use iced::{Element, Length, Color, theme, alignment};

use iced_aw::menu::menu_tree::MenuTree;
use iced_aw::{helpers::menu_tree, menu_tree};

use crate::Message;

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

fn debug_button<'a>(label: &str) -> button::Button<'a, Message, iced::Renderer> {
    labeled_button(label, Message::Debug(label.into()))
}

/*pub fn sub_menu<'a>(
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

fn debug_sub_menu<'a>(
    label: &str,
    children: Vec<MenuTree<'a, Message, iced::Renderer>>,
) -> MenuTree<'a, Message, iced::Renderer> {
    sub_menu(label, Message::Debug(label.into()), children)
}

fn debug_item<'a>(label: &str) -> MenuTree<'a, Message, iced::Renderer> {
    menu_tree!(debug_button(label).width(Length::Fill).height(Length::Fill))
}*/

fn build_menu_items<'a>() -> Vec<MenuTree<'a, Message, iced::Renderer>> {
    let menu_items = vec![
        // labeled_button(label, Message::OpenFolder(label.into()))
        labeled_button(&String::from("Open Folder"), Message::OpenFolder ),
        labeled_button(&String::from("Open File"), Message::OpenFile ),
        labeled_button(&String::from("Close"), Message::Close ),
    ];
    menu_items.into_iter().map(|item| menu_tree!(item.width(Length::Fill).height(Length::Fill))).collect()
}

pub fn menu_1<'a>() -> MenuTree<'a, Message, iced::Renderer> {
    /*let sub_1 = debug_sub_menu(
        "A sub menu",
        vec![
            debug_item("Item"),
            debug_item("Item"),
            // sub_2,
            debug_item("Item"),
            debug_item("Item"),
            debug_item("Item"),
        ],
    )
    .width(220);*/

    let c = build_menu_items();

    let root = menu_tree(
        debug_button("File"),
        c
    )
    .width(110);

    root
}
