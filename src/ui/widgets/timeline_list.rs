use std::collections::HashMap;

use iced::widget::{column, container, scrollable, text};
use iced::{border, Background, Color, Element, Length, Theme};

use crate::app::message::AppMessage;
use crate::app::state::TimelineState;
use crate::ui::widgets::message_bubble;

const C_CHAT_BG: Color = Color::from_rgb8(0x18, 0x1A, 0x1F);
const MEDIA_PREVIEW_WINDOW: usize = 12;

/// Render the scrollable timeline in a WeChat-like visual style.
pub fn view<'a>(
    channel_id: u64,
    channel_type: i32,
    timeline: &'a TimelineState,
    opened_menu_message_id: Option<u64>,
    image_cache: &'a HashMap<u64, iced::widget::image::Handle>,
    peer_last_read_pts: Option<u64>,
    playing_voice_message_id: Option<u64>,
) -> Element<'a, AppMessage> {
    let mut list = column!().spacing(14).padding([12, 18]);

    if timeline.is_loading_more {
        list = list.push(centered_tip("Loading history..."));
    }

    if !timeline.items.is_empty() {
        let start_media_index = timeline.items.len().saturating_sub(MEDIA_PREVIEW_WINDOW);
        for (index, message) in timeline.items.iter().enumerate() {
            let render_media_preview = index >= start_media_index;
            list = list.push(message_bubble::view(
                message,
                opened_menu_message_id,
                render_media_preview,
                image_cache,
                peer_last_read_pts,
                playing_voice_message_id,
            ));
        }
    }

    scrollable(container(list).width(Length::Fill))
        .height(Length::Fill)
        .width(Length::Fill)
        .anchor_bottom()
        .on_scroll(move |viewport| {
            let mut relative_y = viewport.relative_offset().y;
            if relative_y.is_nan() {
                relative_y = 0.0;
            }

            // anchor_bottom() does not invert relative_offset: 0.0=top, 1.0=bottom.
            let at_bottom = relative_y >= 0.98;
            let near_top = relative_y <= 0.02;

            AppMessage::ViewportChanged {
                channel_id,
                channel_type,
                at_bottom,
                near_top,
            }
        })
        .style(timeline_scroll_style)
        .into()
}

fn centered_tip(label: &str) -> Element<'_, AppMessage> {
    container(
        text(label)
            .size(12)
            .color(Color::from_rgb8(0x8F, 0x96, 0xA0)),
    )
    .width(Length::Fill)
    .center_x(Length::Fill)
    .into()
}

fn timeline_scroll_style(theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let mut style = scrollable::default(theme, status);
    style.container = container::Style {
        background: Some(Background::Color(C_CHAT_BG)),
        ..container::Style::default()
    };
    style.vertical_rail.background = Some(Background::Color(Color::from_rgba8(0, 0, 0, 0.0)));
    style.vertical_rail.scroller.background = Background::Color(Color::from_rgb8(0x4A, 0x50, 0x58));
    style.vertical_rail.scroller.border = border::rounded(6.0);
    style
}
