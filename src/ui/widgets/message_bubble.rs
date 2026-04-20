use std::collections::HashMap;

use iced::widget::{button, column, container, mouse_area, row, text};
use iced::{alignment, border, Background, Color, Element, Length, Theme};

use crate::app::message::AppMessage;
use crate::presentation::vm::{MessageSendStateVm, MessageVm};
use crate::ui::widgets::bubbles::{self, BubbleCtx, MessageRenderType};

fn resolve_outgoing_status(message: &MessageVm, peer_last_read_pts: Option<u64>) -> &'static str {
    // 1. 已读：自己发的消息 pts <= 对方 read cursor
    if let (Some(pts), Some(peer_pts)) = (message.pts, peer_last_read_pts) {
        if pts <= peer_pts {
            return "已读";
        }
    }
    // 2. 已送达：对端在线 session 已 ack
    if message.delivered {
        return "已送达";
    }
    // 3. 已发送：有 server_message_id 说明服务端已确认
    if message.server_message_id.is_some() {
        return "已发送";
    }
    // 4. 根据发送状态判断
    match &message.send_state {
        Some(MessageSendStateVm::Queued) => "待发送",
        Some(MessageSendStateVm::Sending) => "发送中",
        Some(MessageSendStateVm::Retrying) => "发送中",
        Some(MessageSendStateVm::Sent) => "已发送",
        Some(MessageSendStateVm::FailedRetryable { .. }) => "发送失败",
        Some(MessageSendStateVm::FailedPermanent { .. }) => "发送失败",
        None => "已发送",
    }
}

fn status_color(label: &str) -> Color {
    match label {
        "已读" => Color::from_rgba8(0x07, 0x7C, 0x3A, 0.85),
        "已送达" => Color::from_rgba8(0x1A, 0x6B, 0x9C, 0.85),
        "发送失败" => Color::from_rgba8(0xCC, 0x33, 0x33, 0.85),
        _ => Color::from_rgba8(0x1A, 0x20, 0x18, 0.70),
    }
}

/// Render one timeline row. 顶部按 `MessageRenderType` 分派：
/// - `Revoked` / `System` 走各自的整行布局，跳过 footer / 菜单 / 撤回 / 重试
/// - `Bubble` 走常规 avatar + 气泡外壳
pub fn view<'a>(
    message: &'a MessageVm,
    render_media_preview: bool,
    image_cache: &'a HashMap<u64, iced::widget::image::Handle>,
    peer_last_read_pts: Option<u64>,
    playing_voice_message_id: Option<u64>,
) -> Element<'a, AppMessage> {
    match bubbles::render_type(message) {
        MessageRenderType::Revoked => return revoked_row(message),
        MessageRenderType::System => return system_row(message),
        MessageRenderType::Bubble => {}
    }

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
    let footer: Element<'_, AppMessage> = if message.is_own {
        let status_label = resolve_outgoing_status(message, peer_last_read_pts);
        container(
            row![
                text(time_text)
                    .size(11)
                    .color(Color::from_rgba8(0x1A, 0x20, 0x18, 0.62)),
                text(status_label)
                    .size(11)
                    .color(status_color(status_label)),
            ]
            .spacing(8),
        )
        .width(Length::Fill)
        .align_x(alignment::Horizontal::Left)
        .into()
    } else {
        container(
            text(time_text)
                .size(11)
                .color(Color::from_rgb8(0x8E, 0x95, 0x9E)),
        )
        .width(Length::Fill)
        .align_x(alignment::Horizontal::Right)
        .into()
    };

    let ctx = BubbleCtx {
        bubble_text,
        is_own: message.is_own,
        render_media_preview,
        image_cache,
        playing_voice_message_id,
    };

    let rendered = bubbles::render(message, &ctx);
    let content = rendered.element;
    let is_attachment = rendered.kind.is_attachment();

    let bubble_content = column![content, footer].spacing(8);

    let bubble = container(bubble_content)
        .max_width(560.0)
        .padding([10, 13])
        .style(move |_| container::Style {
            background: Some(Background::Color(bubble_bg)),
            border: border::rounded(7.0),
            ..container::Style::default()
        });

    let mut body = column![bubble].spacing(4);

    // 重试按钮保留为气泡旁常驻状态图标等价物（与 privchat-app 对齐，不放在菜单里）。
    if message.is_own {
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

/// 已撤回消息：保留 avatar + 气泡外壳，仅渲染灰色提示文案；右键仍触发菜单（仅"本地删除"）。
fn revoked_row<'a>(message: &'a MessageVm) -> Element<'a, AppMessage> {
    let bubble_bg = if message.is_own {
        Color::from_rgb8(0x95, 0xEC, 0x69)
    } else {
        Color::from_rgb8(0x2F, 0x33, 0x3A)
    };
    let content_color = if message.is_own {
        Color::from_rgb8(0x2D, 0x36, 0x2D)
    } else {
        Color::from_rgb8(0xB7, 0xBE, 0xC8)
    };

    let bubble = container(text("消息已撤回").size(14).color(content_color))
        .max_width(560.0)
        .padding([10, 13])
        .style(move |_| container::Style {
            background: Some(Background::Color(bubble_bg)),
            border: border::rounded(7.0),
            ..container::Style::default()
        });

    let avatar = avatar_chip(message.is_own);
    let inner = if message.is_own {
        row![fill(), bubble, avatar]
            .spacing(10)
            .align_y(alignment::Vertical::Top)
    } else {
        row![avatar, bubble, fill()]
            .spacing(10)
            .align_y(alignment::Vertical::Top)
    };

    let container_row = container(inner).width(Length::Fill);
    mouse_area(container_row)
        .on_right_press(AppMessage::ShowTextMenu {
            message_id: message.message_id,
            text: String::new(),
        })
        .into()
}

/// 系统消息：居中浅色药丸，无 avatar / 气泡背景 / footer / 菜单。
/// 文案直接使用 `message.body`（由 SDK / 服务端格式化好的字符串）。
fn system_row<'a>(message: &'a MessageVm) -> Element<'a, AppMessage> {
    let pill = container(
        text(&message.body)
            .size(12)
            .color(Color::from_rgb8(0xB0, 0xB8, 0xC2)),
    )
    .padding([4, 10])
    .style(|_| container::Style {
        background: Some(Background::Color(Color::from_rgba8(
            0x30, 0x36, 0x3F, 0.6,
        ))),
        border: border::rounded(10.0),
        ..container::Style::default()
    });

    container(pill)
        .width(Length::Fill)
        .align_x(alignment::Horizontal::Center)
        .into()
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

    match chrono::DateTime::<chrono::Utc>::from_timestamp(seconds, 0) {
        Some(dt) => {
            let local = dt.with_timezone(&chrono::Local);
            local.format("%H:%M").to_string()
        }
        None => "--:--".to_string(),
    }
}
