use iced::widget::{column, container, text};
use iced::{Background, Color, Element, Length};

use crate::app::message::AppMessage;
use crate::app::state::SettingsState;

/// Render the settings screen.
pub fn view(_settings: &SettingsState) -> Element<'_, AppMessage> {
    container(
        column![
            text("设置")
                .size(24)
                .color(Color::from_rgb8(0xEE, 0xF2, 0xF8)),
            text("设置页面骨架已接入，可继续补充具体设置项。")
                .size(14)
                .color(Color::from_rgb8(0x9A, 0xA1, 0xAB)),
        ]
        .spacing(10),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding([30, 26])
    .style(|_| container::Style {
        background: Some(Background::Color(Color::from_rgb8(0x18, 0x1A, 0x1F))),
        ..container::Style::default()
    })
    .into()
}
