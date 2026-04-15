use std::collections::HashMap;

use iced::widget::{button, column, container, image, mouse_area, row, text};
use iced::{alignment, border, Background, Color, Element, Length, Theme};
use privchat_protocol::message::ContentMessageType;

use crate::app::message::AppMessage;
use crate::presentation::vm::{MessageSendStateVm, MessageVm};

const IMAGE_MESSAGE_TYPE: i32 = ContentMessageType::Image as i32;
const FILE_MESSAGE_TYPE: i32 = ContentMessageType::File as i32;
const VIDEO_MESSAGE_TYPE: i32 = ContentMessageType::Video as i32;

fn send_state_label_zh(state: &MessageSendStateVm, read_hint: bool) -> &'static str {
    match state {
        MessageSendStateVm::Queued => "发送中",
        MessageSendStateVm::Sending => "发送中",
        MessageSendStateVm::Retrying => "发送中",
        MessageSendStateVm::Sent => {
            if read_hint {
                "已读"
            } else {
                "已发送"
            }
        }
        MessageSendStateVm::FailedRetryable { .. } => "发送失败",
        MessageSendStateVm::FailedPermanent { .. } => "发送失败",
    }
}

/// Render one timeline row in a WeChat-like bubble style.
pub fn view<'a>(
    message: &'a MessageVm,
    opened_menu_message_id: Option<u64>,
    render_media_preview: bool,
    image_cache: &'a HashMap<u64, iced::widget::image::Handle>,
) -> Element<'a, AppMessage> {
    let bubble_bg = if message.is_own {
        Color::from_rgb8(0x95, 0xEC, 0x69)
    } else {
        Color::from_rgb8(0x2F, 0x33, 0x3A)
    };
    let bubble_text = if message.is_own {
        Color::from_rgb8(0x11, 0x1B, 0x12)
    } else {
        Color::from_rgb8(0xEC, 0xEF, 0xF3)
    };

    let time_text = format_message_time(message.created_at);
    let footer: Option<Element<'_, AppMessage>> = if message.is_deleted {
        None
    } else if message.is_own {
        // Any message resolved to a server id should be displayed as sent.
        // This avoids stale local failure labels after eventual queue success.
        let status_label = if message.server_message_id.is_some() {
            "已发送"
        } else {
            message
                .send_state
                .as_ref()
                .map(|state| send_state_label_zh(state, message.pts.is_some()))
                .unwrap_or("已发送")
        };

        Some(
            container(
                row![
                    text(time_text)
                        .size(11)
                        .color(Color::from_rgba8(0x1A, 0x20, 0x18, 0.62)),
                    text(status_label)
                        .size(11)
                        .color(Color::from_rgba8(0x1A, 0x20, 0x18, 0.70)),
                ]
                .spacing(8),
            )
            .width(Length::Fill)
            .align_x(alignment::Horizontal::Left)
            .into(),
        )
    } else {
        Some(
            container(
                text(time_text)
                    .size(11)
                    .color(Color::from_rgb8(0x8E, 0x95, 0x9E)),
            )
            .width(Length::Fill)
            .align_x(alignment::Horizontal::Right)
            .into(),
        )
    };

    let content: Element<'_, AppMessage> = if message.is_deleted {
        text("消息已撤回")
        .size(14)
        .color(if message.is_own {
            Color::from_rgb8(0x2D, 0x36, 0x2D)
        } else {
            Color::from_rgb8(0xB7, 0xBE, 0xC8)
        })
        .into()
    } else if message.message_type == IMAGE_MESSAGE_TYPE {
        if render_media_preview
            && (message.local_thumbnail_path.is_some()
                || message.media_local_path.is_some()
                || message.media_url.is_some())
        {
            // Use cached decoded Handle if available; otherwise show placeholder
            let preview: Element<'_, AppMessage> = if let Some(handle) = image_cache.get(&message.message_id) {
                image(handle.clone())
                    .width(Length::Fixed(220.0))
                    .height(Length::Fixed(160.0))
                    .content_fit(iced::ContentFit::Cover)
                    .into()
            } else {
                container(
                    text("[加载中...]")
                        .size(14)
                        .color(Color::from_rgb8(0xE3, 0xE8, 0xEE)),
                )
                .width(Length::Fixed(220.0))
                .height(Length::Fixed(80.0))
                .align_x(alignment::Horizontal::Center)
                .align_y(alignment::Vertical::Center)
                .style(|_| container::Style {
                    background: Some(Background::Color(Color::from_rgb8(0x2B, 0x31, 0x39))),
                    border: border::rounded(8.0),
                    ..container::Style::default()
                })
                .into()
            };
            button(preview)
                .style(navless_button_style)
                .on_press(AppMessage::OpenImagePreview {
                    message_id: message.message_id,
                    original_path: message.media_local_path.clone(),
                    thumbnail_path: message.local_thumbnail_path.clone(),
                    media_url: message.media_url.clone(),
                    file_id: message.media_file_id,
                    created_at: message.created_at,
                })
                .into()
        } else if message.local_thumbnail_path.is_some()
            || message.media_local_path.is_some()
            || message.media_url.is_some()
        {
            container(
                text("[图片]")
                    .size(14)
                    .color(Color::from_rgb8(0xE3, 0xE8, 0xEE)),
            )
            .width(Length::Fixed(220.0))
            .height(Length::Fixed(64.0))
            .align_x(alignment::Horizontal::Center)
            .align_y(alignment::Vertical::Center)
            .style(|_| container::Style {
                background: Some(Background::Color(Color::from_rgb8(0x2B, 0x31, 0x39))),
                border: border::rounded(8.0),
                ..container::Style::default()
            })
            .into()
        } else {
            text(&message.body)
                .size(15)
                .line_height(iced::widget::text::LineHeight::Relative(1.28))
                .color(bubble_text)
                .into()
        }
    } else if matches!(message.message_type, FILE_MESSAGE_TYPE | VIDEO_MESSAGE_TYPE) {
        let file_action_label = if message.media_local_path.is_some() {
            "打开"
        } else {
            "下载"
        };
        let action_button = button(text(file_action_label).size(12))
            .style(retry_button_style)
            .on_press(AppMessage::OpenAttachment {
                message_id: message.message_id,
                created_at: message.created_at,
                local_path: message.media_local_path.clone(),
                file_id: message.media_file_id,
                filename: Some(message.body.clone()),
            });

        let mut meta_col = column![
            action_button,
            text(&message.body)
                .size(15)
                .line_height(iced::widget::text::LineHeight::Relative(1.28))
                .color(bubble_text)
        ]
        .spacing(4);
        if let Some(size) = message.media_file_size {
            meta_col = meta_col.push(text(format_file_size(size)).size(12).color(
                if message.is_own {
                    Color::from_rgb8(0x22, 0x2A, 0x20)
                } else {
                    Color::from_rgb8(0xB8, 0xC0, 0xCC)
                },
            ));
        }
        button(meta_col)
            .style(navless_button_style)
            .on_press(AppMessage::OpenAttachment {
                message_id: message.message_id,
                created_at: message.created_at,
                local_path: message.media_local_path.clone(),
                file_id: message.media_file_id,
                filename: Some(message.body.clone()),
            })
            .into()
    } else {
        text(&message.body)
            .size(15)
            .line_height(iced::widget::text::LineHeight::Relative(1.28))
            .color(bubble_text)
            .into()
    };

    let mut bubble_content = column![content].spacing(8);
    if let Some(footer) = footer {
        bubble_content = bubble_content.push(footer);
    }

    let bubble = container(bubble_content)
        .max_width(560.0)
        .padding([10, 13])
        .style(move |_| container::Style {
            background: Some(Background::Color(bubble_bg)),
            border: border::rounded(7.0),
            ..container::Style::default()
        });

    let mut body = column![bubble].spacing(4);

    let is_attachment = matches!(
        message.message_type,
        IMAGE_MESSAGE_TYPE | FILE_MESSAGE_TYPE | VIDEO_MESSAGE_TYPE
    );
    let show_attachment_menu = opened_menu_message_id == Some(message.message_id) && is_attachment;
    if show_attachment_menu {
        body = body.push(
            row![
                small_menu_button("打开", AppMessage::AttachmentMenuOpen),
                small_menu_button("打开所在目录", AppMessage::AttachmentMenuOpenFolder),
                small_menu_button("另存为", AppMessage::AttachmentMenuSaveAs),
            ]
            .spacing(6),
        );
    } else if opened_menu_message_id == Some(message.message_id) && !message.is_deleted {
        body = body.push(row![small_menu_button("复制", AppMessage::TextMenuCopy)].spacing(6));
    }
    if message.is_own {
        if !message.is_deleted {
            if let Some(server_message_id) = message.server_message_id {
                body = body.push(
                    button(text("撤回").size(11))
                        .style(retry_button_style)
                        .on_press(AppMessage::RevokeMessagePressed {
                            channel_id: message.channel_id,
                            channel_type: message.channel_type,
                            server_message_id,
                        }),
                );
            }
        }
        if let Some(send_state) = &message.send_state {
            if matches!(send_state, MessageSendStateVm::FailedRetryable { .. })
                && message.server_message_id.is_none()
            {
                if let Some(client_txn_id) = message.client_txn_id {
                    body = body.push(
                        button(text("重试").size(11))
                            .style(retry_button_style)
                            .on_press(AppMessage::RetrySendPressed {
                                channel_id: message.channel_id,
                                channel_type: message.channel_type,
                                client_txn_id,
                            }),
                    );
                }
            }
        }
    }

    let avatar = avatar_chip(message.is_own);
    let row = if message.is_own {
        row![fill(), body, avatar]
            .spacing(10)
            .align_y(alignment::Vertical::Top)
    } else {
        row![avatar, body, fill()]
            .spacing(10)
            .align_y(alignment::Vertical::Top)
    };

    let container_row = container(row).width(Length::Fill);
    if message.is_deleted {
        return container_row.into();
    }

    if is_attachment {
        mouse_area(container_row)
            .on_right_press(AppMessage::ShowAttachmentMenu {
                message_id: message.message_id,
                created_at: message.created_at,
                local_path: message.media_local_path.clone(),
                file_id: message.media_file_id,
                filename: message.body.clone(),
            })
            .into()
    } else {
        mouse_area(container_row)
            .on_right_press(AppMessage::ShowTextMenu {
                message_id: message.message_id,
                text: message.body.clone(),
            })
            .into()
    }
}

fn avatar_chip(is_own: bool) -> Element<'static, AppMessage> {
    const C_LIST_AVATAR: Color = Color::from_rgb8(0x5A, 0x6F, 0x86);

    let (bg, label) = if is_own {
        (Color::from_rgb8(0x3E, 0x56, 0x78), "ME")
    } else {
        (C_LIST_AVATAR, "OT")
    };

    container(
        text(label)
            .size(10)
            .color(Color::from_rgb8(0xEC, 0xF0, 0xF4)),
    )
    .width(Length::Fixed(38.0))
    .height(Length::Fixed(38.0))
    .align_x(alignment::Horizontal::Center)
    .align_y(alignment::Vertical::Center)
    .style(move |_| container::Style {
        background: Some(Background::Color(bg)),
        border: border::rounded(6.0),
        ..container::Style::default()
    })
    .into()
}

fn retry_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => Color::from_rgb8(0x44, 0x49, 0x51),
        _ => Color::from_rgb8(0x34, 0x38, 0x3F),
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::from_rgb8(0xB9, 0xC0, 0xCA),
        border: border::rounded(6.0),
        shadow: Default::default(),
        snap: true,
    }
}

fn navless_button_style(_theme: &Theme, _status: button::Status) -> button::Style {
    button::Style {
        background: None,
        text_color: Color::TRANSPARENT,
        border: border::width(0.0),
        shadow: Default::default(),
        snap: true,
    }
}

fn small_menu_button<'a>(label: &'a str, msg: AppMessage) -> Element<'a, AppMessage> {
    button(text(label).size(11))
        .style(retry_button_style)
        .on_press(msg)
        .into()
}

fn format_file_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

fn fill() -> Element<'static, AppMessage> {
    container(text("")).width(Length::Fill).into()
}

fn format_message_time(created_at: i64) -> String {
    if created_at <= 0 {
        return "--:--".to_string();
    }

    let seconds = if created_at > 1_000_000_000_000 {
        created_at / 1000
    } else {
        created_at
    };

    let normalized = ((seconds % 86_400) + 86_400) % 86_400;
    let hour = normalized / 3_600;
    let minute = (normalized % 3_600) / 60;
    format!("{hour:02}:{minute:02}")
}
