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



use iced::widget::{container, row, button, text, svg,};
use iced::alignment::{self, Horizontal, Vertical};
use iced::{Element, Length, Color, theme, Border};
use iced::border::Radius;
//use iced_aw::menu::menu_tree::MenuTree;
//use iced_aw::{helpers::menu_tree, menu_tree};
use iced_aw::menu::{self, Item, Menu};
//use iced_aw::{menu, menu_bar, menu_items};
use iced_aw::{menu_bar, menu_items};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonClass {
    Transparent,
    Labeled,
    Nothing,
}


use iced::Theme as BaseTheme;

pub struct CustomTheme(pub BaseTheme);

impl button::Catalog for CustomTheme {
    type Class<'a> = ButtonClass;

    fn default<'a>() -> Self::Class<'a> {
        ButtonClass::Transparent
    }

    fn style(&self, class: &Self::Class<'_>, status: button::Status) -> button::Style {
        let palette = self.0.extended_palette(); // Access the inner theme's palette

        match class {
            ButtonClass::Transparent => match status {
                button::Status::Active | button::Status::Hovered => button::Style {
                    text_color: palette.background.base.text,
                    background: Some(Color::TRANSPARENT.into()),
                    //border_radius: 4.0,
                    border: Border {
                        color: Color::TRANSPARENT,
                        width: 0.0,
                        radius: Radius {
                            top_left: 4.0,
                            top_right: 4.0,
                            bottom_right: 4.0,
                            bottom_left: 4.0,
                        },
                    },
                    ..Default::default()
                },
                _ => button::Style::default(),
            },
            ButtonClass::Labeled => match status {
                button::Status::Active => button::Style {
                    text_color: palette.primary.weak.text,
                    background: Some(palette.primary.weak.color.into()),
                    //border_radius: 4.0,
                    border: Border {
                        color: Color::TRANSPARENT,
                        width: 0.0,
                        radius: Radius {
                            top_left: 4.0,
                            top_right: 4.0,
                            bottom_right: 4.0,
                            bottom_left: 4.0,
                        },
                    },
                    ..Default::default()
                },
                button::Status::Hovered => button::Style {
                    background: Some(palette.primary.strong.color.into()),
                    text_color: palette.primary.strong.text,
                    ..Default::default()
                },
                _ => button::Style::default(),
            },
            ButtonClass::Nothing => button::Style::default(),
        }
    }
}


use iced::widget::button::{Style};
use iced::Theme;
//use iced::border::Radius;

impl<'a> From<ButtonClass> for Box<dyn Fn(&Theme, button::Status) -> Style + 'a> {
    fn from(class: ButtonClass) -> Self {
        Box::new(move |theme: &Theme, status: button::Status| match class {
            ButtonClass::Transparent => Style {
                text_color: theme.extended_palette().background.base.text,
                background: Some(iced::Color::TRANSPARENT.into()),
                border: iced::Border {
                    color: iced::Color::TRANSPARENT,
                    width: 0.0,
                    radius: Radius {
                        top_left: 4.0,
                        top_right: 4.0,
                        bottom_right: 4.0,
                        bottom_left: 4.0,
                    },
                },
                ..Default::default()
            },
            ButtonClass::Labeled => match status {
                button::Status::Active => Style {
                    text_color: theme.extended_palette().primary.weak.text,
                    background: Some(theme.extended_palette().primary.weak.color.into()),
                    border: iced::Border {
                        color: iced::Color::TRANSPARENT,
                        width: 0.0,
                        radius: Radius {
                            top_left: 4.0,
                            top_right: 4.0,
                            bottom_right: 4.0,
                            bottom_left: 4.0,
                        },
                    },
                    ..Default::default()
                },
                button::Status::Hovered => Style {
                    background: Some(theme.extended_palette().primary.strong.color.into()),
                    text_color: theme.extended_palette().primary.strong.text,
                    ..Default::default()
                },
                _ => Style::default(),
            },
            ButtonClass::Nothing => Style::default(),
        })
    }
}


/*fn base_button<'a>(
    content: impl Into<Element<'a, Message>>,
    msg: Message,
) -> button::Button<'a, Message> {
    button(content)
        .padding([4, 8])
        //.style(iced::theme::Button::Custom(Box::new(ButtonStyle {})))
        //.style(iced::theme::Button::Custom(Box::new(ButtonStyle)))
        .style(Theme::Custom(Box::new(ButtonStyle)))
        .on_press(msg)
}

fn labeled_button <'a>(
    label: &str,
    text_size: u16,
    msg: Message
) -> button::Button<'a, Message> {
    button(text(label)
        .size(text_size)
        .width(Length::Fill)
        .height(Length::Fill)
        .vertical_alignment(alignment::Vertical::Center)
    )
    .padding([4, 8])
    //.style(iced::theme::Button::Custom(Box::new(ButtonStyle {})))
    .style(Theme::Custom(Box::new(ButtonStyle)))
    .on_press(msg)
}

fn nothing_button <'a>(label: &str, text_size: u16) -> button::Button<'a, Message> {
    labeled_button(label, text_size, Message::Nothing)
}*/
fn base_button<'a>(
    content: impl Into<Element<'a, Message>>,
    msg: Message,
) -> button::Button<'a, Message> {
    button(content)
        .padding([4, 8])
        .class(ButtonClass::Transparent)
        .on_press(msg)
}

fn labeled_button<'a>(
    label: &'a str,
    text_size: u16,
    msg: Message,
) -> button::Button<'a, Message> {
    // 0.10.0
    /*button(
        text(label)
            .size(text_size)
            .width(Length::Fill)
            .height(Length::Fill)
            .vertical_alignment(alignment::Vertical::Center),
    )
    .padding([4, 8])
    .class(ButtonClass::Labeled)
    .on_press(msg)*/

    // 0.13.1
    button(
        container(
            text(label)
                .size(text_size)
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .align_x(Horizontal::Center) // Align horizontally
        .align_y(Vertical::Center),  // Align vertically
    )
    .padding([4, 8])
    .class(ButtonClass::Labeled)
    .on_press(msg)

}

fn nothing_button<'a>(label: &'a str, text_size: u16) -> button::Button<'a, Message> {
    button(
        container(
            text(label)
                .size(text_size)
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .align_x(Horizontal::Center) // Align horizontally
        .align_y(Vertical::Center),  // Align vertically
    )
    .padding([4, 8])
    .class(ButtonClass::Nothing)
}

/*pub fn sub_menu_msg<'a>(
    label: &str,
    msg: Message,
    children: Vec<MenuTree<'a, Message>>,
) -> MenuTree<'a, Message> {
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


fn build_menu_items_v1<'a>() -> Vec<MenuTree<'a, Message>> {
    let menu_items = vec![
        labeled_button(&String::from("Open Folder (Alt+1 or 2)"), MENU_ITEM_FONT_SIZE, Message::OpenFolder(0) ),
        labeled_button(&String::from("Open File (Alt+Ctrl+1 or 2)"), MENU_ITEM_FONT_SIZE, Message::OpenFile(0) ),
        labeled_button(&String::from("Close (Ctrl+W)"), MENU_ITEM_FONT_SIZE, Message::Close ),
        labeled_button(&String::from("Quit (Ctrl+Q)"), MENU_ITEM_FONT_SIZE, Message::Quit ),
    ];
    menu_items.into_iter().map(|item| menu_tree!(item.width(Length::Fill).height(Length::Fill))).collect()
}

pub fn menu_3<'a>(app: &DataViewer) -> MenuTree<'a, Message> {
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

pub fn menu_1<'a>(_app: &DataViewer) -> MenuTree<'a, Message> {
    let c = build_menu_items_v1();
    let root = menu_tree(
        nothing_button("File", MENU_FONT_SIZE),
        c
    );

    root
}
*/


/*pub fn menu_3<'a>(app: &DataViewer) -> Menu<'a, Message> {
    let pane_layout_submenu = Menu::new(menu_items!(
        (labeled_button("Single Pane (Ctrl+1)", MENU_ITEM_FONT_SIZE, Message::TogglePaneLayout(PaneLayout::SinglePane)))
        (labeled_button("Dual Pane (Ctrl+2)", MENU_ITEM_FONT_SIZE, Message::TogglePaneLayout(PaneLayout::DualPane)))
    ))
    .max_width(180.0)
    .spacing(5.0);

    let controls_menu = menu_items!(
        (toggler::Toggler::new(Some("Toggle Slider (Space)".into()), app.is_slider_dual, Message::ToggleSliderType).padding([4, 8]))
        (toggler::Toggler::new(Some("Toggle Footer (Tab)".into()), app.show_footer, Message::ToggleFooter).padding([4, 8]))
    );

    Menu::new(menu_items!(
        (nothing_button("Controls", MENU_FONT_SIZE))
        (menu_bar!(("Pane Layout", pane_layout_submenu)))
        // TODO: Add this back
        //(menu_bar!(controls_menu))
    ))
    .max_width(240.0)
    .spacing(5.0)
}

fn build_menu_items_v1<'a>() -> Vec<Item<'a, Message>> {
    menu_items!(
        (labeled_button("Open Folder (Alt+1 or 2)", MENU_ITEM_FONT_SIZE, Message::OpenFolder(0)))
        (labeled_button("Open File (Alt+Ctrl+1 or 2)", MENU_ITEM_FONT_SIZE, Message::OpenFile(0)))
        (labeled_button("Close (Ctrl+W)", MENU_ITEM_FONT_SIZE, Message::Close))
        (labeled_button("Quit (Ctrl+Q)", MENU_ITEM_FONT_SIZE, Message::Quit))
    )
}

pub fn menu_1<'a>(_app: &DataViewer) -> Menu<'a, Message> {
    let menu_items = build_menu_items_v1();
    Menu::new(menu_items)
        .max_width(240.0)
        .spacing(5.0)
}*/
pub fn menu_3<'a>(app: &DataViewer) -> Menu<'a, Message, iced::Theme, iced::Renderer> {
    let pane_layout_submenu = Menu::new(menu_items!(
        (labeled_button(
            "Single Pane (Ctrl+1)",
            MENU_ITEM_FONT_SIZE,
            Message::TogglePaneLayout(PaneLayout::SinglePane)
        ))
        (labeled_button(
            "Dual Pane (Ctrl+2)",
            MENU_ITEM_FONT_SIZE,
            Message::TogglePaneLayout(PaneLayout::DualPane)
        ))
    ))
    .max_width(180.0)
    .spacing(5.0);

    /*let controls_menu = Menu::new(menu_items!(
        (toggler::Toggler::new(
            Some("Toggle Slider (Space)".into()),
            app.is_slider_dual,
            Message::ToggleSliderType
        )
        .padding([4, 8]))
        (toggler::Toggler::new(
            Some("Toggle Footer (Tab)".into()),
            app.show_footer,
            Message::ToggleFooter
        )
        .padding([4, 8]))
    ))
    .max_width(180.0)
    .spacing(5.0);*/
    let controls_menu = Menu::new(menu_items!(
        (container(
            toggler::Toggler::new(
                Some("Toggle Slider (Space)".into()),
                app.is_slider_dual,
                Message::ToggleSliderType,
            )
        )
        .padding([4, 8]))
        (container(
            toggler::Toggler::new(
                Some("Toggle Footer (Tab)".into()),
                app.show_footer,
                Message::ToggleFooter,
            )
        )
        .padding([4, 8]))
    ))
    .max_width(180.0)
    .spacing(5.0);


    Menu::new(menu_items!(
        (nothing_button("Controls", MENU_FONT_SIZE))
        (menu_bar!(("Pane Layout", pane_layout_submenu)))
        (menu_bar!(("Controls", controls_menu)))
    ))
    .max_width(240.0)
    .spacing(5.0)
}

fn build_menu_items_v1<'a>() -> Vec<Item<'a, Message, iced::Theme, iced::Renderer>> {
    menu_items!(
        (labeled_button(
            "Open Folder (Alt+1 or 2)",
            MENU_ITEM_FONT_SIZE,
            Message::OpenFolder(0)
        ))
        (labeled_button(
            "Open File (Alt+Ctrl+1 or 2)",
            MENU_ITEM_FONT_SIZE,
            Message::OpenFile(0)
        ))
        (labeled_button("Close (Ctrl+W)", MENU_ITEM_FONT_SIZE, Message::Close))
        (labeled_button("Quit (Ctrl+Q)", MENU_ITEM_FONT_SIZE, Message::Quit))
    )
}

pub fn menu_1<'a>(
    _app: &DataViewer,
) -> Menu<'a, Message, iced::Theme, iced::Renderer> {
    let menu_items = build_menu_items_v1();
    Menu::new(menu_items).max_width(240.0).spacing(5.0)
}