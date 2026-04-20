use iced::widget::{button, container, image, text};
use iced::{alignment, border, Background, Color, Element, Length, Theme};

use crate::app::message::AppMessage;
use crate::presentation::vm::MessageVm;

use super::BubbleCtx;

pub fn view<'a>(message: &'a MessageVm, ctx: &BubbleCtx<'a>) -> Element<'a, AppMessage> {
    let has_media = message.local_thumbnail_path.is_some()
        || message.media_local_path.is_some()
        || message.media_url.is_some();

    if ctx.render_media_preview && has_media {
        let preview: Element<'_, AppMessage> =
            if let Some(handle) = ctx.image_cache.get(&message.message_id) {
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
    } else if has_media {
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
            .color(ctx.bubble_text)
            .into()
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
