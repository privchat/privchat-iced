use iced::widget::{button, column, container, row, text};
use iced::{alignment, border, Background, Color, Element, Length, Theme};

use crate::app::message::AppMessage;
use crate::presentation::vm::{MessageSendStateVm, MessageVm};

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
pub fn view(message: &MessageVm) -> Element<'_, AppMessage> {
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
        let status_label = message
            .send_state
            .as_ref()
            .map(|state| send_state_label_zh(state, message.pts.is_some()))
            .unwrap_or("已发送");

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

    let bubble = container(
        column![
            text(&message.body)
                .size(15)
                .line_height(iced::widget::text::LineHeight::Relative(1.28))
                .color(bubble_text),
            footer,
        ]
        .spacing(8),
    )
    .max_width(560.0)
    .padding([10, 13])
    .style(move |_| container::Style {
        background: Some(Background::Color(bubble_bg)),
        border: border::rounded(7.0),
        ..container::Style::default()
    });

    let mut body = column![bubble].spacing(4);
    if message.is_own {
        if let Some(send_state) = &message.send_state {
            if matches!(send_state, MessageSendStateVm::FailedRetryable { .. }) {
                if let Some(client_txn_id) = message.client_txn_id {
                    body = body.push(
                        button(text("Retry").size(11))
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

    container(row).width(Length::Fill).into()
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

    let normalized = ((seconds % 86_400) + 86_400) % 86_400;
    let hour = normalized / 3_600;
    let minute = (normalized % 3_600) / 60;
    format!("{hour:02}:{minute:02}")
}
