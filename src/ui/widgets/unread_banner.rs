use iced::widget::{container, text};
use iced::{border, Background, Color, Element, Length};

use crate::app::message::AppMessage;
use crate::presentation::vm::UnreadMarkerVm;

/// Render unread banner in timeline.
pub fn view(unread: &UnreadMarkerVm) -> Element<'_, AppMessage> {
    if unread.unread_count == 0 && !unread.has_unread_below_viewport {
        return container(text("")).into();
    }

    let label = if unread.unread_count > 0 {
        format!("{} unread messages", unread.unread_count)
    } else {
        "New messages below".to_string()
    };

    container(
        container(
            text(label)
                .size(13)
                .color(Color::from_rgb8(0xD5, 0xDB, 0xE1)),
        )
        .padding([6, 12])
        .style(|_| container::Style {
            background: Some(Background::Color(Color::from_rgb8(0x33, 0x38, 0x40))),
            border: border::rounded(10.0),
            ..container::Style::default()
        }),
    )
    .width(Length::Fill)
    .center_x(Length::Fill)
    .into()
}
