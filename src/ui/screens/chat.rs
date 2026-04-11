use iced::widget::{column, container, image, mouse_area, row, stack, text};
use iced::{alignment, border, Background, Color, Element, Length};

use crate::app::message::AppMessage;
use crate::app::state::ChatScreenState;
use crate::presentation::vm::PresenceVm;
use crate::ui::icons::{self, Icon};
use crate::ui::widgets::{composer, timeline_list, unread_banner};

const C_HEADER_BG: Color = Color::from_rgb8(0x1A, 0x1D, 0x22);
const C_CHAT_BG: Color = Color::from_rgb8(0x18, 0x1A, 0x1F);
const C_COMPOSER_BG: Color = Color::from_rgb8(0x14, 0x17, 0x1B);
const C_DIVIDER: Color = Color::from_rgb8(0x2A, 0x2E, 0x34);
const C_STATUS_ONLINE: Color = Color::from_rgb8(0x22, 0xC5, 0x5E);
const C_STATUS_OFFLINE: Color = Color::from_rgb8(0x95, 0x9D, 0xA6);
const COMPOSER_HEIGHT: f32 = 184.0;
const EMOJI_POPUP_BOTTOM_OFFSET: f32 = 160.0;

/// Render WeChat-like right chat pane.
pub fn view<'a>(
    chat: &'a ChatScreenState,
    title: &'a str,
    presence: Option<&'a PresenceVm>,
    typing_hint: Option<&'a str>,
) -> Element<'a, AppMessage> {
    let header_title = column![
        text(title)
            .size(17)
            .color(Color::from_rgb8(0xF0, 0xF2, 0xF4)),
        presence_status_text(presence, typing_hint),
    ]
    .spacing(3);
    let header = container(
        row![
            header_title,
            container(
                row![
                    icons::render(
                        Icon::BubbleOutline,
                        26.0,
                        Color::from_rgb8(0xA7, 0xAD, 0xB6)
                    ),
                    icons::render(Icon::ChevronDown, 17.0, Color::from_rgb8(0xA7, 0xAD, 0xB6)),
                    icons::render(Icon::Phone, 26.0, Color::from_rgb8(0xA7, 0xAD, 0xB6)),
                    icons::render(Icon::Dots, 23.0, Color::from_rgb8(0xA7, 0xAD, 0xB6)),
                ]
                .spacing(11)
                .align_y(alignment::Vertical::Center),
            )
            .width(Length::Fill)
            .align_x(alignment::Horizontal::Right)
            .align_y(alignment::Vertical::Center)
        ]
        .height(Length::Fill)
        .align_y(alignment::Vertical::Center),
    )
    .height(Length::Fixed(58.0))
    .padding([0, 16])
    .style(|_| container::Style {
        background: Some(Background::Color(C_HEADER_BG)),
        border: border::width(1.0).color(C_DIVIDER),
        ..container::Style::default()
    });

    let body = container(
        column![
            unread_banner::view(&chat.unread_marker),
            timeline_list::view(
                chat.channel_id,
                chat.channel_type,
                &chat.timeline,
                chat.attachment_menu.as_ref().map(|m| m.message_id),
            ),
        ]
        .height(Length::Fill),
    )
    .height(Length::Fill)
    .style(|_| container::Style {
        background: Some(Background::Color(C_CHAT_BG)),
        ..container::Style::default()
    });

    let composer = container(composer::view(&chat.composer))
        .height(Length::Fixed(COMPOSER_HEIGHT))
        .style(|_| container::Style {
            background: Some(Background::Color(C_COMPOSER_BG)),
            border: border::width(1.0).color(C_DIVIDER),
            ..container::Style::default()
        });

    let content: Element<'_, AppMessage> = if chat.composer.emoji_picker_open {
        stack![
            column![header, body, composer]
                .width(Length::Fill)
                .height(Length::Fill),
            mouse_area(container(text("")).width(Length::Fill).height(Length::Fill))
                .on_press(AppMessage::DismissEmojiPicker),
            container(
                column![
                    container(text("")).height(Length::Fill),
                    row![
                        composer::emoji_picker_popup(),
                        container(text("")).width(Length::Fill)
                    ]
                    .width(Length::Fill),
                    container(text("")).height(Length::Fixed(EMOJI_POPUP_BOTTOM_OFFSET))
                ]
                .width(Length::Fill)
                .height(Length::Fill)
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .padding([0, 14])
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else {
        column![header, body, composer]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    };

    let content: Element<'_, AppMessage> = if let Some(path) = &chat.preview_image_path {
        stack![
            content,
            mouse_area(
                container(text(""))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(|_| container::Style {
                        background: Some(Background::Color(Color::from_rgba8(0, 0, 0, 0.68))),
                        ..container::Style::default()
                    })
            )
            .on_press(AppMessage::CloseImagePreview),
            container(
                image(path.clone())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .content_fit(iced::ContentFit::Contain)
            )
            .padding(20)
            .width(Length::Fill)
            .height(Length::Fill)
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else {
        content
    };

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn presence_status_text<'a>(
    presence: Option<&'a PresenceVm>,
    typing_hint: Option<&'a str>,
) -> Element<'a, AppMessage> {
    if let Some(text) = typing_hint.filter(|value| !value.trim().is_empty()) {
        return iced::widget::text(text)
            .size(12)
            .color(Color::from_rgb8(0x22, 0xC5, 0x5E))
            .into();
    }

    let Some(presence) = presence else {
        return container(text("")).into();
    };

    let (label, color) = if presence.is_online {
        ("在线".to_string(), C_STATUS_ONLINE)
    } else if presence.last_seen_at > 0 {
        (
            format!("最近在线 {}", format_presence_time(presence.last_seen_at)),
            C_STATUS_OFFLINE,
        )
    } else {
        ("离线".to_string(), C_STATUS_OFFLINE)
    };

    text(label).size(12).color(color).into()
}

fn format_presence_time(timestamp_ms: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(timestamp_ms)
        .map(|dt| {
            dt.with_timezone(&chrono::Local)
                .format("%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "--:--".to_string())
}
