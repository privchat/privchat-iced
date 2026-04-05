use iced::widget::{button, column, container, scrollable, text};
use iced::{border, Background, Color, Element, Length, Theme};

use crate::app::message::AppMessage;
use crate::app::state::TimelineState;
use crate::ui::widgets::message_bubble;

const C_CHAT_BG: Color = Color::from_rgb8(0x18, 0x1A, 0x1F);

/// Render the scrollable timeline in a WeChat-like visual style.
pub fn view(
    channel_id: u64,
    channel_type: i32,
    timeline: &TimelineState,
) -> Element<'_, AppMessage> {
    let mut list = column!().spacing(14).padding([12, 18]);

    if timeline.is_loading_more {
        list = list.push(centered_tip("Loading history..."));
    } else if timeline.has_more_before {
        list = list.push(
            container(
                button(text("Load older messages").size(12))
                    .style(load_older_button_style)
                    .on_press(AppMessage::LoadOlderTriggered {
                        channel_id,
                        channel_type,
                    }),
            )
            .width(Length::Fill)
            .center_x(Length::Fill),
        );
    }

    if !timeline.items.is_empty() {
        for (index, message) in timeline.items.iter().enumerate() {
            if show_timestamp(index) {
                list = list.push(timestamp_separator(index));
            }
            list = list.push(message_bubble::view(message));
        }
    }

    scrollable(container(list).width(Length::Fill))
        .height(Length::Fill)
        .width(Length::Fill)
        .style(timeline_scroll_style)
        .into()
}

fn centered_tip(label: &str) -> Element<'_, AppMessage> {
    container(
        text(label)
            .size(12)
            .color(Color::from_rgb8(0x8F, 0x96, 0xA0)),
    )
    .width(Length::Fill)
    .center_x(Length::Fill)
    .into()
}

fn show_timestamp(index: usize) -> bool {
    index == 0 || index % 2 == 0
}

fn timestamp_separator(index: usize) -> Element<'static, AppMessage> {
    container(
        text(mock_timestamp(index))
            .size(12)
            .color(Color::from_rgb8(0x8D, 0x94, 0x9E)),
    )
    .width(Length::Fill)
    .center_x(Length::Fill)
    .into()
}

fn load_older_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let fg = match status {
        button::Status::Hovered | button::Status::Pressed => Color::from_rgb8(0xB8, 0xC0, 0xCA),
        _ => Color::from_rgb8(0x96, 0x9E, 0xA8),
    };
    button::Style {
        background: None,
        text_color: fg,
        border: border::rounded(4.0),
        shadow: Default::default(),
        snap: true,
    }
}

fn timeline_scroll_style(theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let mut style = scrollable::default(theme, status);
    style.container = container::Style {
        background: Some(Background::Color(C_CHAT_BG)),
        ..container::Style::default()
    };
    style.vertical_rail.background = Some(Background::Color(Color::from_rgba8(0, 0, 0, 0.0)));
    style.vertical_rail.scroller.background = Background::Color(Color::from_rgb8(0x4A, 0x50, 0x58));
    style.vertical_rail.scroller.border = border::rounded(6.0);
    style
}

fn mock_timestamp(index: usize) -> &'static str {
    const TIMES: [&str; 7] = [
        "Thursday 14:56",
        "Thursday 17:08",
        "Friday 23:42",
        "Yesterday 20:34",
        "Yesterday 21:10",
        "Yesterday 23:08",
        "Today 10:12",
    ];
    TIMES[index % TIMES.len()]
}
