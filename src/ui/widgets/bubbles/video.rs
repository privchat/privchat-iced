use iced::widget::{button, column, container, image, row, stack, text};
use iced::{alignment, border, Background, Color, Element, Length, Theme};

use crate::app::message::AppMessage;
use crate::presentation::vm::MessageVm;

use super::BubbleCtx;

/// 视频气泡：缩略图 + 时长/大小 + 播放按钮。
/// 点击任意处触发下载并用系统默认播放器播放（已缓存则直接打开）。
pub fn view<'a>(message: &'a MessageVm, ctx: &BubbleCtx<'a>) -> Element<'a, AppMessage> {
    // `message.body` for a video is display text (e.g. "[视频]"), not a real filename,
    // and `message.media_url` on a video carries the thumbnail URL (.png/.jpg). Neither
    // is a usable filename_hint — leave it `None` and let the handler derive it from the
    // resolved video download URL.
    let open_msg = AppMessage::OpenVideo {
        message_id: message.message_id,
        created_at: message.created_at,
        local_path: message.media_local_path.clone(),
        file_id: message.media_file_id,
        filename: None,
    };

    // thumb_status=3: 协议层无缩略图，直接渲染类型化占位（不显示 loading）
    let thumb_none = message.thumb_status == 3;
    let thumbnail: Element<'_, AppMessage> =
        if ctx.render_media_preview && !thumb_none {
            if let Some(handle) = ctx.image_cache.get(&message.message_id) {
                image(handle.clone())
                    .width(Length::Fixed(220.0))
                    .height(Length::Fixed(160.0))
                    .content_fit(iced::ContentFit::Cover)
                    .into()
            } else {
                container(
                    text("[视频缩略图加载中...]")
                        .size(13)
                        .color(Color::from_rgb8(0xE3, 0xE8, 0xEE)),
                )
                .width(Length::Fixed(220.0))
                .height(Length::Fixed(160.0))
                .align_x(alignment::Horizontal::Center)
                .align_y(alignment::Vertical::Center)
                .style(|_| container::Style {
                    background: Some(Background::Color(Color::from_rgb8(0x2B, 0x31, 0x39))),
                    border: border::rounded(8.0),
                    ..container::Style::default()
                })
                .into()
            }
        } else {
            container(
                text("[视频]")
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
        };

    let play_badge = container(text("▶").size(22).color(Color::WHITE))
        .width(Length::Fixed(44.0))
        .height(Length::Fixed(44.0))
        .align_x(alignment::Horizontal::Center)
        .align_y(alignment::Vertical::Center)
        .style(|_| container::Style {
            background: Some(Background::Color(Color { a: 0.55, ..Color::BLACK })),
            border: border::rounded(22.0),
            ..container::Style::default()
        });

    let badge_row = row![play_badge]
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(alignment::Vertical::Center);
    let badge_centered = container(badge_row)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(alignment::Horizontal::Center)
        .align_y(alignment::Vertical::Center);

    let stacked: Element<'_, AppMessage> = stack![thumbnail, badge_centered].into();

    let mut meta: Vec<Element<'_, AppMessage>> = Vec::new();
    if let Some(size) = message.media_file_size {
        meta.push(
            text(format_file_size(size))
                .size(12)
                .color(if ctx.is_own {
                    Color::from_rgb8(0x22, 0x2A, 0x20)
                } else {
                    Color::from_rgb8(0xB8, 0xC0, 0xCC)
                })
                .into(),
        );
    }

    let body: Element<'_, AppMessage> = if meta.is_empty() {
        stacked
    } else {
        let mut col = column![stacked].spacing(4);
        for el in meta {
            col = col.push(el);
        }
        col.into()
    };

    button(body)
        .style(navless_button_style)
        .on_press(open_msg)
        .into()
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
