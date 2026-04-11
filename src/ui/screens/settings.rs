use iced::widget::{button, checkbox, column, container, text};
use iced::{Background, Color, Element, Length, border};

use crate::app::message::AppMessage;
use crate::app::state::SettingsState;

/// Render the settings screen.
pub fn view(settings: &SettingsState) -> Element<'_, AppMessage> {
    container(
        column![
            text("设置")
                .size(24)
                .color(Color::from_rgb8(0xEE, 0xF2, 0xF8)),
            text("基础设置")
                .size(14)
                .color(Color::from_rgb8(0x9A, 0xA1, 0xAB)),
            container(
                column![
                    checkbox(settings.notification_sound_enabled)
                        .label("新消息提示音")
                        .on_toggle(|_| AppMessage::ToggleNotificationSound)
                        .size(16),
                    button("打开日志窗口")
                        .on_press(AppMessage::SettingsMenuOpenLogs),
                    text(
                        settings
                            .logs_feedback
                            .as_deref()
                            .unwrap_or("日志窗口支持复制全部、清空、导出。"),
                    )
                    .size(12)
                    .color(Color::from_rgb8(0x9A, 0xA1, 0xAB)),
                ]
                .spacing(10),
            )
            .padding(12)
            .style(|_| container::Style {
                background: Some(Background::Color(Color::from_rgb8(0x22, 0x26, 0x2E))),
                border: border::rounded(8.0)
                    .width(1.0)
                    .color(Color::from_rgb8(0x34, 0x3A, 0x44)),
                ..container::Style::default()
            }),
        ]
        .spacing(12),
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
