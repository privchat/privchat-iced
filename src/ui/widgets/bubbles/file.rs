use iced::widget::{button, column, text};
use iced::{border, Background, Color, Element, Theme};

use crate::app::message::AppMessage;
use crate::presentation::vm::MessageVm;

use super::BubbleCtx;

pub fn view<'a>(message: &'a MessageVm, ctx: &BubbleCtx<'a>) -> Element<'a, AppMessage> {
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
            .color(ctx.bubble_text)
    ]
    .spacing(4);
    if let Some(size) = message.media_file_size {
        meta_col = meta_col.push(text(format_file_size(size)).size(12).color(
            if ctx.is_own {
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
