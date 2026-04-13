use iced::widget::{button, column, container, image, mouse_area, row, stack, text};
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

    let content: Element<'_, AppMessage> = if let Some(pending) = &chat.composer.pending_attachment
    {
        let title = if pending.is_image {
            "发送图片"
        } else {
            "发送文件"
        };
        let preview: Element<'_, AppMessage> = if pending.is_image {
            container(
                image(pending.path.clone())
                    .width(Length::Fixed(300.0))
                    .height(Length::Fixed(180.0))
                    .content_fit(iced::ContentFit::Contain),
            )
            .width(Length::Fill)
            .align_x(alignment::Horizontal::Center)
            .into()
        } else {
            container(text("📎").size(34).color(Color::from_rgb8(0xAF, 0xB6, 0xC1)))
                .width(Length::Fill)
                .align_x(alignment::Horizontal::Center)
                .into()
        };

        stack![
            content,
            mouse_area(
                container(text(""))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(|_| container::Style {
                        background: Some(Background::Color(Color::from_rgba8(0, 0, 0, 0.62))),
                        ..container::Style::default()
                    })
            )
            .on_press(AppMessage::ComposerAttachmentSendCanceled),
            container(
                column![
                    text(title).size(18).color(Color::from_rgb8(0xEA, 0xEE, 0xF3)),
                    preview,
                    text(&pending.filename)
                        .size(14)
                        .color(Color::from_rgb8(0xC1, 0xC8, 0xD2)),
                    row![
                        button(text("取消").size(14))
                            .padding([8, 18])
                            .on_press(AppMessage::ComposerAttachmentSendCanceled),
                        button(text("发送").size(14))
                            .padding([8, 18])
                            .on_press(AppMessage::ComposerAttachmentSendConfirmed),
                    ]
                    .spacing(10)
                ]
                .spacing(12)
                .align_x(alignment::Horizontal::Center)
            )
            .padding([14, 18])
            .width(Length::Fixed(360.0))
            .style(|_| container::Style {
                background: Some(Background::Color(Color::from_rgb8(0x25, 0x2A, 0x31))),
                border: border::width(1.0)
                    .color(Color::from_rgb8(0x3A, 0x41, 0x4B))
                    .rounded(10.0),
                ..container::Style::default()
            })
            .align_x(alignment::Horizontal::Center)
            .align_y(alignment::Vertical::Center)
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
    let typing_text = typing_hint.filter(|value| !value.trim().is_empty());
    let presence_status = presence.map(presence_status_bucket);

    match (presence_status, typing_text) {
        (Some((status_label, status_color)), Some(typing_label)) => row![
            text(status_label).size(12).color(status_color),
            text("·").size(12).color(Color::from_rgb8(0x6B, 0x73, 0x7D)),
            text(typing_label).size(12).color(C_STATUS_ONLINE),
        ]
        .spacing(6)
        .align_y(alignment::Vertical::Center)
        .into(),
        (Some((status_label, status_color)), None) => {
            text(status_label).size(12).color(status_color).into()
        }
        (None, Some(typing_label)) => text(typing_label).size(12).color(C_STATUS_ONLINE).into(),
        (None, None) => text("").size(12).color(C_STATUS_OFFLINE).into(),
    }
}

fn presence_status_bucket(presence: &PresenceVm) -> (String, Color) {
    if presence.is_online {
        return ("在线".to_string(), C_STATUS_ONLINE);
    }

    let last_seen_at = presence.last_seen_at;
    if last_seen_at <= 0 {
        return ("很久没有上线".to_string(), C_STATUS_OFFLINE);
    }

    // last_seen_at is Unix seconds from the server; compare in seconds
    let now = chrono::Utc::now().timestamp();
    let elapsed = now.saturating_sub(last_seen_at);
    let day: i64 = 24 * 60 * 60;

    let label = if elapsed < day {
        "不久前在线"
    } else if elapsed < 7 * day {
        "1天前在线"
    } else if elapsed < 30 * day {
        "7天前在线"
    } else if elapsed < 90 * day {
        "30天前在线"
    } else {
        "很久没有上线"
    };

    (label.to_string(), C_STATUS_OFFLINE)
}
