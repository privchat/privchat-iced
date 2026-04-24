use std::collections::HashMap;

use iced::widget::{column, container, scrollable, text};
use iced::{border, Background, Color, Element, Length, Theme};

use crate::app::message::AppMessage;
use crate::app::state::TimelineState;
use crate::presentation::vm::{MessageVm, OpenToken, ReactionChipVm};
use crate::ui::widgets::message_bubble::{self, ReplyPreview};

const C_CHAT_BG: Color = Color::from_rgb8(0x18, 0x1A, 0x1F);
const MEDIA_PREVIEW_WINDOW: usize = 12;

/// Render the scrollable timeline in a WeChat-like visual style.
pub fn view<'a>(
    channel_id: u64,
    channel_type: i32,
    timeline: &'a TimelineState,
    image_cache: &'a HashMap<u64, iced::widget::image::Handle>,
    peer_last_read_pts: Option<u64>,
    playing_voice_message_id: Option<u64>,
    message_reactions: &'a HashMap<u64, Vec<ReactionChipVm>>,
    reaction_picker_for: Option<u64>,
    open_token: OpenToken,
) -> Element<'a, AppMessage> {
    let mut list = column!().spacing(14).padding([12, 18]);

    if timeline.is_loading_more {
        list = list.push(centered_tip("Loading history..."));
    }

    if !timeline.items.is_empty() {
        let reply_preview_lookup: HashMap<u64, ReplyPreview> = timeline
            .items
            .iter()
            .filter_map(build_reply_preview)
            .collect();
        let start_media_index = timeline.items.len().saturating_sub(MEDIA_PREVIEW_WINDOW);
        for (index, message) in timeline.items.iter().enumerate() {
            let render_media_preview = index >= start_media_index;
            let reply_preview = message
                .reply_to_server_message_id
                .map(|smid| {
                    match reply_preview_lookup.get(&smid).cloned() {
                        Some(preview) => {
                            tracing::info!(
                                target: "reply_debug",
                                "reply_preview hit: msg_id={} reply_to_smid={} preview={}",
                                message.message_id,
                                smid,
                                &preview.body.chars().take(60).collect::<String>()
                            );
                            preview
                        }
                        None => {
                            tracing::info!(
                                target: "reply_debug",
                                "reply_preview miss: msg_id={} reply_to_smid={} lookup_keys={:?}",
                                message.message_id,
                                smid,
                                reply_preview_lookup.keys().collect::<Vec<_>>()
                            );
                            ReplyPreview::deleted()
                        }
                    }
                });
            let reactions = message_reactions
                .get(&message.message_id)
                .map(|v| v.as_slice());
            let picker_open = reaction_picker_for == Some(message.message_id);
            list = list.push(message_bubble::view(
                message,
                render_media_preview,
                image_cache,
                peer_last_read_pts,
                playing_voice_message_id,
                reply_preview,
                reactions,
                picker_open,
                open_token,
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

fn build_reply_preview(message: &MessageVm) -> Option<(u64, ReplyPreview)> {
    let smid = message.server_message_id?;
    Some((smid, ReplyPreview::from_message(message)))
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
