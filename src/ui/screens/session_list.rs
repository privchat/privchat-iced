use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{alignment, border, Background, Color, Element, Length, Theme};

use crate::app::message::AppMessage;
use crate::app::state::{SessionListItemState, SessionListState};
use crate::ui::icons::{self, Icon};

const C_PANEL_BG: Color = Color::from_rgb8(0x2B, 0x2E, 0x34);
const C_SEARCH_BG: Color = Color::from_rgb8(0x24, 0x27, 0x2D);
const C_SEARCH_BORDER: Color = Color::from_rgb8(0x3A, 0x3F, 0x47);
const C_LIST_HOVER: Color = Color::from_rgb8(0x37, 0x3B, 0x42);
const C_LIST_SELECTED: Color = Color::from_rgb8(0x4C, 0x50, 0x57);

/// Render WeChat-like session/conversation panel.
pub fn view(
    session_list: &SessionListState,
    active_chat: Option<(u64, i32)>,
) -> Element<'_, AppMessage> {
    let mut list = column!().spacing(0);

    if session_list.items.is_empty() {
        list = list.push(
            container(
                text("暂无会话")
                    .size(14)
                    .color(Color::from_rgb8(0xA7, 0xAD, 0xB5)),
            )
            .width(Length::Fill)
            .padding([20, 16]),
        );
    } else {
        for (index, item) in session_list.items.iter().enumerate() {
            let selected = active_chat.is_some_and(|(channel_id, channel_type)| {
                channel_id == item.channel_id && channel_type == item.channel_type
            });
            list = list.push(conversation_item(item, index, selected));
        }
    }

    column![
        search_bar(),
        scrollable(list)
            .height(Length::Fill)
            .style(session_scroll_style),
    ]
    .height(Length::Fill)
    .into()
}

fn search_bar() -> Element<'static, AppMessage> {
    let search_input = text_input("Search", "")
        .on_input(|_| AppMessage::Noop)
        .padding([8, 10])
        .size(14)
        .style(search_input_style)
        .width(Length::Fill);

    let input_with_icon = container(
        row![
            icons::render(Icon::Search, 16.0, Color::from_rgb8(0x8D, 0x95, 0x9E)),
            search_input
        ]
        .spacing(8)
        .align_y(alignment::Vertical::Center),
    )
    .style(|_| container::Style {
        background: Some(Background::Color(C_SEARCH_BG)),
        border: border::rounded(7.0),
        ..container::Style::default()
    })
    .padding([0, 10]);

    let plus = button(icons::render(
        Icon::Plus,
        21.0,
        Color::from_rgb8(0x9E, 0xA6, 0xAF),
    ))
    .padding([8, 8])
    .style(plus_button_style);

    container(row![input_with_icon, plus].spacing(10))
        .padding([10, 12])
        .style(|_| container::Style {
            background: Some(Background::Color(C_PANEL_BG)),
            ..container::Style::default()
        })
        .into()
}

fn conversation_item(
    item: &SessionListItemState,
    index: usize,
    selected: bool,
) -> Element<'_, AppMessage> {
    let display_title = truncate_nickname(&item.title, 9);

    let avatar = container(text(""))
        .width(Length::Fixed(40.0))
        .height(Length::Fixed(40.0))
        .style(|_| container::Style {
            background: Some(Background::Color(Color::from_rgb8(0x5A, 0x6F, 0x86))),
            border: border::rounded(6.0),
            ..container::Style::default()
        });

    let row = row![
        avatar,
        column![
            row![
                text(display_title)
                    .size(14)
                    .wrapping(iced::widget::text::Wrapping::None)
                    .color(Color::from_rgb8(0xEA, 0xEE, 0xF4)),
                container(
                    text(mock_time(index))
                        .size(12)
                        .wrapping(iced::widget::text::Wrapping::None)
                        .color(Color::from_rgb8(0x9A, 0xA1, 0xAB))
                )
                .width(Length::Fill)
                .align_x(alignment::Horizontal::Right),
            ],
            text(&item.subtitle)
                .size(12)
                .color(Color::from_rgb8(0xA4, 0xAB, 0xB4)),
        ]
        .spacing(5)
        .width(Length::Fill),
    ]
    .spacing(9)
    .align_y(alignment::Vertical::Center);

    button(container(row).width(Length::Fill))
        .width(Length::Fill)
        .padding([10, 12])
        .style(move |_theme: &Theme, status| session_item_style(selected, status))
        .on_press(AppMessage::ConversationSelected {
            channel_id: item.channel_id,
            channel_type: item.channel_type,
        })
        .into()
}

fn session_item_style(selected: bool, status: button::Status) -> button::Style {
    let active_bg = if selected {
        C_LIST_SELECTED
    } else {
        C_PANEL_BG
    };
    let hover_bg = if selected {
        C_LIST_SELECTED
    } else {
        C_LIST_HOVER
    };
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => hover_bg,
        button::Status::Active | button::Status::Disabled => active_bg,
    };

    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::from_rgb8(0xEA, 0xEE, 0xF4),
        border: border::width(0.0),
        shadow: Default::default(),
        snap: true,
    }
}

fn plus_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => Color::from_rgb8(0x41, 0x46, 0x4E),
        _ => Color::from_rgb8(0x33, 0x38, 0x40),
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::from_rgb8(0xB5, 0xBC, 0xC5),
        border: border::rounded(8.0),
        shadow: Default::default(),
        snap: true,
    }
}

fn search_input_style(_theme: &Theme, status: text_input::Status) -> text_input::Style {
    let border_color = match status {
        text_input::Status::Focused { .. } => Color::from_rgb8(0x42, 0x4A, 0x54),
        text_input::Status::Hovered => Color::from_rgb8(0x3B, 0x42, 0x4B),
        text_input::Status::Active | text_input::Status::Disabled => C_SEARCH_BORDER,
    };

    text_input::Style {
        background: Background::Color(C_SEARCH_BG),
        border: border::width(0.0).rounded(7.0).color(border_color),
        icon: Color::from_rgb8(0x8F, 0x96, 0x9F),
        placeholder: Color::from_rgb8(0x8F, 0x96, 0x9F),
        value: Color::from_rgb8(0xD9, 0xDE, 0xE4),
        selection: Color::from_rgb8(0x47, 0x8F, 0x67),
    }
}

fn session_scroll_style(theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let mut style = scrollable::default(theme, status);
    style.container = container::Style {
        background: Some(Background::Color(C_PANEL_BG)),
        ..container::Style::default()
    };
    style.vertical_rail.background = None;
    style.vertical_rail.border = border::width(0.0).rounded(0.0).color(Color::TRANSPARENT);
    style.vertical_rail.scroller.background = Background::Color(Color::from_rgba8(0, 0, 0, 0.0));
    style.vertical_rail.scroller.border = border::width(0.0).rounded(0.0).color(Color::TRANSPARENT);
    style
}

fn mock_time(index: usize) -> &'static str {
    const TIMES: [&str; 12] = [
        "05:35",
        "03:20",
        "Yesterday 23:50",
        "Yesterday 21:41",
        "Yesterday 21:10",
        "Yesterday 20:46",
        "Yesterday 16:31",
        "Yesterday 06:04",
        "Yesterday 03:44",
        "Friday",
        "Thursday",
        "Monday",
    ];
    TIMES[index % TIMES.len()]
}

fn truncate_nickname(value: &str, max_chars: usize) -> String {
    let count = value.chars().count();
    if count <= max_chars {
        return value.to_string();
    }

    let kept: String = value.chars().take(max_chars).collect();
    format!("{kept}...")
}
