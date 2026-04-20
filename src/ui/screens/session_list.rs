use iced::widget::{button, column, container, mouse_area, row, scrollable, stack, text, text_input};
use iced::{alignment, border, Background, Color, Element, Length, Theme};

use crate::app::message::AppMessage;
use crate::app::state::{AppState, SessionContextMenuState, SessionListItemState};
use crate::presentation::vm::PresenceVm;
use crate::ui::icons::{self, Icon};

const C_PANEL_BG: Color = Color::from_rgb8(0x2B, 0x2E, 0x34);
const C_SEARCH_BG: Color = Color::from_rgb8(0x24, 0x27, 0x2D);
const C_SEARCH_BORDER: Color = Color::from_rgb8(0x3A, 0x3F, 0x47);
const C_LIST_HOVER: Color = Color::from_rgb8(0x37, 0x3B, 0x42);
const C_LIST_SELECTED: Color = Color::from_rgb8(0x4C, 0x50, 0x57);
const C_LIST_PINNED: Color = Color::from_rgb8(0x31, 0x35, 0x3C);
const C_ONLINE: Color = Color::from_rgb8(0x22, 0xC5, 0x5E);

/// Render WeChat-like session/conversation panel.
pub fn view(
    state: &AppState,
    active_chat: Option<(u64, i32)>,
    panel_width: f32,
) -> Element<'_, AppMessage> {
    let session_list = &state.session_list;
    let mut list = column!().spacing(0);

    if let Some(error) = &session_list.load_error {
        list = list.push(
            container(
                text(format!("SESSION_LIST_ERR: {error}"))
                    .size(12)
                    .color(Color::from_rgb8(0xD0, 0x6B, 0x6B)),
            )
            .width(Length::Fill)
            .padding([8, 12]),
        );
    }

    let mut ordered: Vec<&SessionListItemState> = session_list.items.iter().collect();
    ordered.sort_by(|a, b| {
        b.is_pinned
            .cmp(&a.is_pinned)
            .then(b.last_msg_timestamp.cmp(&a.last_msg_timestamp))
    });

    if ordered.is_empty() {
        list = list.push(
            container(
                text("暂无会话")
                    .size(14)
                    .color(Color::from_rgb8(0xA7, 0xAD, 0xB5)),
            )
            .width(Length::Fill)
            .padding([20, 16]),
        );
    } else {
        for item in &ordered {
            let selected = active_chat.is_some_and(|(channel_id, channel_type)| {
                channel_id == item.channel_id && channel_type == item.channel_type
            });
            let presence_user_id = item.peer_user_id;
            let presence = presence_user_id.and_then(|user_id| state.presences.get(&user_id));
            let friend_online_fallback = presence_user_id.and_then(|user_id| {
                state
                    .add_friend
                    .friends
                    .iter()
                    .find(|friend| friend.user_id == user_id)
                    .map(|friend| friend.is_online)
            });
            list = list.push(conversation_item(
                item,
                selected,
                panel_width,
                presence,
                friend_online_fallback,
            ));
        }
    }

    let panel: Element<'_, AppMessage> = column![
        search_bar(),
        scrollable(list)
            .height(Length::Fill)
            .style(session_scroll_style),
    ]
    .height(Length::Fill)
    .into();

    // Wrap in mouse_area to track cursor for menu anchoring.
    let tracked_panel: Element<'_, AppMessage> = mouse_area(panel)
        .on_move(AppMessage::SessionListCursorMoved)
        .into();

    if let Some(menu) = &session_list.context_menu {
        let pos = menu.anchor_pos.unwrap_or(iced::Point::ORIGIN);
        let offset_x = pos.x.max(0.0);
        let offset_y = pos.y.max(0.0);
        stack![
            tracked_panel,
            mouse_area(
                container(text(""))
                    .width(Length::Fill)
                    .height(Length::Fill)
            )
            .on_press(AppMessage::DismissSessionContextMenu)
            .on_right_press(AppMessage::DismissSessionContextMenu),
            column![
                container(text("")).height(Length::Fixed(offset_y)),
                row![
                    container(text("")).width(Length::Fixed(offset_x)),
                    context_menu_popup(menu),
                    container(text("")).width(Length::Fill),
                ],
                container(text("")).height(Length::Fill),
            ]
            .width(Length::Fill)
            .height(Length::Fill),
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else {
        tracked_panel
    }
}

fn search_bar() -> Element<'static, AppMessage> {
    let search_input = text_input("搜索", "")
        .on_input(|_| AppMessage::Noop)
        .padding([8, 10])
        .size(14)
        .style(search_input_style)
        .width(Length::Fill);

    let input_with_icon = container(
        row![
            icons::render(Icon::Search, 16.0, Color::from_rgb8(0x8D, 0x95, 0x9E)),
            search_input
        ]
        .spacing(8)
        .align_y(alignment::Vertical::Center),
    )
    .style(|_| container::Style {
        background: Some(Background::Color(C_SEARCH_BG)),
        border: border::rounded(7.0),
        ..container::Style::default()
    })
    .padding([0, 10]);

    let plus = button(icons::render(
        Icon::Plus,
        21.0,
        Color::from_rgb8(0x9E, 0xA6, 0xAF),
    ))
    .padding([8, 8])
    .style(plus_button_style);

    container(row![input_with_icon, plus].spacing(10))
        .padding([10, 12])
        .style(|_| container::Style {
            background: Some(Background::Color(C_PANEL_BG)),
            ..container::Style::default()
        })
        .into()
}

fn conversation_item<'a>(
    item: &'a SessionListItemState,
    selected: bool,
    panel_width: f32,
    presence: Option<&'a PresenceVm>,
    friend_online_fallback: Option<bool>,
) -> Element<'a, AppMessage> {
    let (title_max_chars, subtitle_max_chars) = text_budget_from_panel_width(panel_width);
    let display_title = truncate_single_line(&item.title, title_max_chars);
    let display_subtitle = truncate_single_line(&item.subtitle, subtitle_max_chars);
    let status = resolve_presence_status(presence, friend_online_fallback);
    let is_online = status.as_ref().map(|value| value.is_online).unwrap_or(false);

    let mut title_row = row![
        container(
            text(display_title)
                .size(14)
                .wrapping(iced::widget::text::Wrapping::None)
                .color(Color::from_rgb8(0xEA, 0xEE, 0xF4))
        )
        .width(Length::Fill),
        container(session_item_meta(item))
            .width(Length::Fixed(52.0))
            .align_x(alignment::Horizontal::Right),
    ]
    .spacing(4)
    .align_y(alignment::Vertical::Center);

    if item.is_pinned {
        title_row = row![
            text("📌").size(11),
            title_row,
        ]
        .spacing(4)
        .align_y(alignment::Vertical::Center);
    }

    let mut text_col = column![
        title_row,
        text(display_subtitle)
            .size(12)
            .wrapping(iced::widget::text::Wrapping::None)
            .color(Color::from_rgb8(0xA4, 0xAB, 0xB4)),
    ]
    .spacing(5)
    .width(Length::Fill);

    if let Some(status) = &status {
        text_col = text_col.push(
            text(status.label.clone())
                .size(11)
                .wrapping(iced::widget::text::Wrapping::None)
                .color(status.color),
        );
    }

    let row = row![
        avatar_with_badge(item.unread_count, is_online),
        text_col,
    ]
    .spacing(9)
    .align_y(alignment::Vertical::Center);

    let channel_id = item.channel_id;
    let channel_type = item.channel_type;
    let is_pinned = item.is_pinned;

    let item_button = button(container(row).width(Length::Fill))
        .width(Length::Fill)
        .padding([10, 12])
        .style(move |_theme: &Theme, status| session_item_style(selected, is_pinned, status))
        .on_press(AppMessage::ConversationSelected {
            channel_id,
            channel_type,
        });

    mouse_area(item_button)
        .on_right_press(AppMessage::SessionListItemRightClicked {
            channel_id,
            channel_type,
            is_pinned,
        })
        .into()
}

fn context_menu_popup<'a>(menu: &SessionContextMenuState) -> Element<'a, AppMessage> {
    let channel_id = menu.channel_id;
    let channel_type = menu.channel_type;
    let pin_label = if menu.is_pinned { "取消置顶" } else { "置顶" };
    let pin_next = !menu.is_pinned;

    let items = column![
        context_menu_item(
            pin_label.to_string(),
            AppMessage::PinChannelPressed {
                channel_id,
                channel_type,
                pinned: pin_next,
            },
        ),
        context_menu_item(
            "隐藏".to_string(),
            AppMessage::HideChannelPressed {
                channel_id,
                channel_type,
            },
        ),
        context_menu_item(
            "删除".to_string(),
            AppMessage::DeleteChannelPressed {
                channel_id,
                channel_type,
            },
        ),
    ]
    .spacing(1);

    container(items)
        .width(Length::Fixed(140.0))
        .padding(4)
        .style(|_| container::Style {
            background: Some(Background::Color(Color::from_rgb8(0x2A, 0x2F, 0x37))),
            border: border::width(1.0)
                .color(Color::from_rgb8(0x3D, 0x44, 0x4D))
                .rounded(8.0),
            ..container::Style::default()
        })
        .into()
}

fn context_menu_item<'a>(label: String, msg: AppMessage) -> Element<'a, AppMessage> {
    button(text(label).size(13).color(Color::from_rgb8(0xE0, 0xE4, 0xEA)))
        .width(Length::Fill)
        .padding([6, 12])
        .style(context_menu_item_style)
        .on_press(msg)
        .into()
}

fn context_menu_item_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => {
            Some(Background::Color(Color::from_rgb8(0x36, 0x3C, 0x44)))
        }
        _ => None,
    };
    button::Style {
        background: bg,
        text_color: Color::from_rgb8(0xE0, 0xE4, 0xEA),
        border: border::rounded(4.0),
        shadow: Default::default(),
        snap: true,
    }
}

fn session_item_style(selected: bool, is_pinned: bool, status: button::Status) -> button::Style {
    let active_bg = if selected {
        C_LIST_SELECTED
    } else if is_pinned {
        C_LIST_PINNED
    } else {
        C_PANEL_BG
    };
    let hover_bg = if selected {
        C_LIST_SELECTED
    } else {
        C_LIST_HOVER
    };
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => hover_bg,
        button::Status::Active | button::Status::Disabled => active_bg,
    };

    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::from_rgb8(0xEA, 0xEE, 0xF4),
        border: border::width(0.0),
        shadow: Default::default(),
        snap: true,
    }
}

fn plus_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => Color::from_rgb8(0x41, 0x46, 0x4E),
        _ => Color::from_rgb8(0x33, 0x38, 0x40),
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::from_rgb8(0xB5, 0xBC, 0xC5),
        border: border::rounded(8.0),
        shadow: Default::default(),
        snap: true,
    }
}

fn search_input_style(_theme: &Theme, status: text_input::Status) -> text_input::Style {
    let border_color = match status {
        text_input::Status::Focused { .. } => Color::from_rgb8(0x42, 0x4A, 0x54),
        text_input::Status::Hovered => Color::from_rgb8(0x3B, 0x42, 0x4B),
        text_input::Status::Active | text_input::Status::Disabled => C_SEARCH_BORDER,
    };

    text_input::Style {
        background: Background::Color(C_SEARCH_BG),
        border: border::width(0.0).rounded(7.0).color(border_color),
        icon: Color::from_rgb8(0x8F, 0x96, 0x9F),
        placeholder: Color::from_rgb8(0x8F, 0x96, 0x9F),
        value: Color::from_rgb8(0xD9, 0xDE, 0xE4),
        selection: Color::from_rgb8(0x47, 0x8F, 0x67),
    }
}

fn session_scroll_style(theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let mut style = scrollable::default(theme, status);
    style.container = container::Style {
        background: Some(Background::Color(C_PANEL_BG)),
        ..container::Style::default()
    };
    style.vertical_rail.background = None;
    style.vertical_rail.border = border::width(0.0).rounded(0.0).color(Color::TRANSPARENT);
    style.vertical_rail.scroller.background = Background::Color(Color::from_rgba8(0, 0, 0, 0.0));
    style.vertical_rail.scroller.border = border::width(0.0).rounded(0.0).color(Color::TRANSPARENT);
    style
}

fn session_item_meta(item: &SessionListItemState) -> Element<'static, AppMessage> {
    let (time_text, time_color) = match format_last_msg_time(item.last_msg_timestamp) {
        Ok(value) => (value, Color::from_rgb8(0x9A, 0xA1, 0xAB)),
        Err(err) => (err.to_string(), Color::from_rgb8(0xD0, 0x6B, 0x6B)),
    };

    column![text(time_text)
        .size(12)
        .wrapping(iced::widget::text::Wrapping::None)
        .color(time_color)]
    .spacing(6)
    .align_x(alignment::Horizontal::Right)
    .into()
}

fn unread_badge(unread_count: u32) -> Element<'static, AppMessage> {
    let label = if unread_count > 99 {
        "99+".to_string()
    } else {
        unread_count.to_string()
    };

    container(text(label).size(10).color(Color::WHITE))
        .padding([2, 6])
        .style(|_| container::Style {
            background: Some(Background::Color(Color::from_rgb8(0xEA, 0x4B, 0x52))),
            border: border::rounded(10.0),
            ..container::Style::default()
        })
        .into()
}

fn avatar_with_badge(unread_count: u32, is_online: bool) -> Element<'static, AppMessage> {
    let avatar = container(text(""))
        .width(Length::Fixed(40.0))
        .height(Length::Fixed(40.0))
        .style(|_| container::Style {
            background: Some(Background::Color(Color::from_rgb8(0x5A, 0x6F, 0x86))),
            border: border::rounded(6.0),
            ..container::Style::default()
        });

    let avatar_layer = container(avatar)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(alignment::Horizontal::Left)
        .align_y(alignment::Vertical::Bottom);

    let online_dot = is_online.then(|| {
        container(text(""))
            .width(Length::Fixed(9.0))
            .height(Length::Fixed(9.0))
            .style(|_| container::Style {
                background: Some(Background::Color(C_ONLINE)),
                border: border::rounded(99.0)
                    .width(2.0)
                    .color(Color::from_rgb8(0x2B, 0x2E, 0x34)),
                ..container::Style::default()
            })
    });

    if unread_count == 0 && online_dot.is_none() {
        return container(avatar_layer)
            .width(Length::Fixed(48.0))
            .height(Length::Fixed(44.0))
            .into();
    }

    let mut layers = vec![avatar_layer.into()];

    if let Some(dot) = online_dot {
        layers.push(
            container(dot)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(alignment::Horizontal::Right)
                .align_y(alignment::Vertical::Bottom)
                .into(),
        );
    }

    if unread_count > 0 {
        layers.push(
            container(unread_badge(unread_count))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(alignment::Horizontal::Right)
                .align_y(alignment::Vertical::Top)
                .into(),
        );
    }

    container(stack(layers).width(Length::Fixed(48.0)).height(Length::Fixed(44.0)))
    .width(Length::Fixed(48.0))
    .height(Length::Fixed(44.0))
    .into()
}

struct PresenceDisplay {
    label: String,
    color: Color,
    is_online: bool,
}

fn resolve_presence_status(
    presence: Option<&PresenceVm>,
    friend_online_fallback: Option<bool>,
) -> Option<PresenceDisplay> {
    if let Some(presence) = presence {
        if presence.is_online {
            return Some(PresenceDisplay {
                label: "在线".to_string(),
                color: C_ONLINE,
                is_online: true,
            });
        }
        let last_seen_at = presence.last_seen_at;
        if last_seen_at <= 0 {
            return Some(PresenceDisplay {
                label: "很久没有上线".to_string(),
                color: Color::from_rgb8(0x9A, 0xA1, 0xAB),
                is_online: false,
            });
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

        return Some(PresenceDisplay {
            label: label.to_string(),
            color: Color::from_rgb8(0x9A, 0xA1, 0xAB),
            is_online: false,
        });
    }

    if friend_online_fallback == Some(true) {
        return Some(PresenceDisplay {
            label: "在线".to_string(),
            color: C_ONLINE,
            is_online: true,
        });
    }

    None
}

fn format_last_msg_time(last_msg_timestamp: i64) -> Result<String, &'static str> {
    if last_msg_timestamp <= 0 {
        return Err("TIME_ERR");
    }

    let seconds = if last_msg_timestamp > 1_000_000_000_000 {
        last_msg_timestamp / 1000
    } else {
        last_msg_timestamp
    };

    match chrono::DateTime::<chrono::Utc>::from_timestamp(seconds, 0) {
        Some(dt) => {
            let local = dt.with_timezone(&chrono::Local);
            Ok(local.format("%H:%M").to_string())
        }
        None => Err("TIME_ERR"),
    }
}

fn truncate_single_line(value: &str, max_chars: usize) -> String {
    let total_units = value.chars().map(display_units).sum::<usize>();
    if total_units <= max_chars {
        return value.to_string();
    }

    let mut used = 0usize;
    let mut kept = String::new();
    for ch in value.chars() {
        let units = display_units(ch);
        if used + units > max_chars.saturating_sub(3) {
            break;
        }
        used += units;
        kept.push(ch);
    }

    if kept.is_empty() {
        "...".to_string()
    } else {
        format!("{kept}...")
    }
}

fn text_budget_from_panel_width(panel_width: f32) -> (usize, usize) {
    // Reserve fixed space for avatar, paddings, and time/meta area.
    let text_px = (panel_width - 142.0).max(120.0);
    let title_units = (text_px / 6.8).floor() as usize;
    let subtitle_units = (text_px / 6.2).floor() as usize;
    (title_units.clamp(14, 180), subtitle_units.clamp(18, 220))
}

fn display_units(ch: char) -> usize {
    if ch.is_ascii() {
        1
    } else {
        2
    }
}
