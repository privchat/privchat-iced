use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{alignment, border, Background, Color, Element, Length};

use crate::app::message::AppMessage;
use crate::app::state::ForwardPickerState;
use crate::presentation::vm::{ForwardTarget, FORWARD_MAX_TARGETS, FORWARD_NOTE_MAX};

const C_CARD_BG: Color = Color::from_rgb8(0x25, 0x2A, 0x31);
const C_CARD_BORDER: Color = Color::from_rgb8(0x3A, 0x41, 0x4B);
const C_TEXT_PRIMARY: Color = Color::from_rgb8(0xE0, 0xE4, 0xE9);
const C_TEXT_SECONDARY: Color = Color::from_rgb8(0x8B, 0x93, 0x9E);
const C_SECTION_HEADER: Color = Color::from_rgb8(0xA6, 0xAE, 0xB8);
const C_ACCENT: Color = Color::from_rgb8(0x2F, 0x7F, 0xD6);
const C_ACCENT_DIM: Color = Color::from_rgb8(0x2F, 0x7F, 0xD6);
const C_ROW_HOVER: Color = Color::from_rgb8(0x2F, 0x35, 0x3E);
const C_ROW_SELECTED: Color = Color::from_rgba8(0x2F, 0x7F, 0xD6, 0.18);
const C_ERROR: Color = Color::from_rgb8(0xEA, 0x4B, 0x52);

pub fn view(state: &ForwardPickerState) -> Element<'_, AppMessage> {
    let header = column![
        text("转发消息").size(16).color(C_TEXT_PRIMARY),
        preview_block(&state.source_preview),
    ]
    .spacing(8);

    let search = text_input("搜索", &state.search)
        .on_input(AppMessage::ForwardSearchChanged)
        .padding([8, 10])
        .size(13)
        .width(Length::Fill);

    let list = candidates_list(state);

    let note = text_input("可选：添加备注", &state.note)
        .on_input(AppMessage::ForwardNoteChanged)
        .padding([8, 10])
        .size(13)
        .width(Length::Fill);

    let note_hint = text(format!(
        "{} / {}",
        state.note.chars().count(),
        FORWARD_NOTE_MAX
    ))
    .size(11)
    .color(C_TEXT_SECONDARY);

    let actions = action_row(state);

    let mut body = column![
        header,
        search,
        container(list).height(Length::Fixed(300.0)),
        note,
        row![container(text("")).width(Length::Fill), note_hint]
            .align_y(alignment::Vertical::Center),
    ]
    .spacing(12);

    if let Some(err) = &state.error {
        body = body.push(text(err.clone()).size(12).color(C_ERROR));
    }

    body = body.push(actions);

    container(body.padding(20))
        .max_width(480.0)
        .width(Length::Fixed(480.0))
        .style(|_| container::Style {
            background: Some(Background::Color(C_CARD_BG)),
            border: border::width(1.0).color(C_CARD_BORDER).rounded(12.0),
            ..container::Style::default()
        })
        .into()
}

fn preview_block(preview: &str) -> Element<'_, AppMessage> {
    let text_value = if preview.trim().is_empty() {
        "(无预览)".to_string()
    } else {
        preview.to_string()
    };
    container(text(text_value).size(12).color(C_TEXT_SECONDARY))
        .padding([6, 10])
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(Color::from_rgba8(0, 0, 0, 0.25))),
            border: border::rounded(6.0),
            ..container::Style::default()
        })
        .into()
}

fn candidates_list(state: &ForwardPickerState) -> Element<'_, AppMessage> {
    let query = state.search.trim().to_lowercase();

    let mut col = column![].spacing(2);
    let mut any = false;

    let recent: Vec<_> = state
        .recent_sessions
        .iter()
        .filter(|s| matches_query(&query, &s.title, &s.subtitle))
        .collect();
    if !recent.is_empty() {
        col = col.push(section_header("最近聊天"));
        for item in recent {
            let target = match item.channel_type {
                1 => item.peer_user_id.map(ForwardTarget::DirectMessage),
                2 => Some(ForwardTarget::Group(item.channel_id)),
                _ => None,
            };
            if let Some(target) = target {
                col = col.push(candidate_row(
                    target,
                    &item.title,
                    &item.subtitle,
                    state.is_selected(target),
                ));
                any = true;
            }
        }
    }

    let friends: Vec<_> = state
        .friends
        .iter()
        .filter(|f| matches_query(&query, &f.title, &f.subtitle))
        .collect();
    if !friends.is_empty() {
        col = col.push(section_header("好友"));
        for item in friends {
            let target = ForwardTarget::DirectMessage(item.user_id);
            col = col.push(candidate_row(
                target,
                &item.title,
                &item.subtitle,
                state.is_selected(target),
            ));
            any = true;
        }
    }

    let groups: Vec<_> = state
        .groups
        .iter()
        .filter(|g| matches_query(&query, &g.title, &g.subtitle))
        .collect();
    if !groups.is_empty() {
        col = col.push(section_header("群组"));
        for item in groups {
            let target = ForwardTarget::Group(item.group_id);
            col = col.push(candidate_row(
                target,
                &item.title,
                &item.subtitle,
                state.is_selected(target),
            ));
            any = true;
        }
    }

    if !any {
        col = col.push(
            container(text("未找到匹配项").size(12).color(C_TEXT_SECONDARY))
                .padding([20, 10])
                .width(Length::Fill)
                .center_x(Length::Fill),
        );
    }

    scrollable(col.width(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn matches_query(query: &str, title: &str, subtitle: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    title.to_lowercase().contains(query) || subtitle.to_lowercase().contains(query)
}

fn section_header(label: &str) -> Element<'_, AppMessage> {
    container(text(label.to_string()).size(11).color(C_SECTION_HEADER))
        .padding([8, 10])
        .width(Length::Fill)
        .into()
}

fn candidate_row<'a>(
    target: ForwardTarget,
    title: &str,
    subtitle: &str,
    selected: bool,
) -> Element<'a, AppMessage> {
    let check = container(text(if selected { "✓" } else { "" }).size(13).color(
        if selected {
            Color::WHITE
        } else {
            Color::TRANSPARENT
        },
    ))
    .width(Length::Fixed(18.0))
    .height(Length::Fixed(18.0))
    .align_x(alignment::Horizontal::Center)
    .align_y(alignment::Vertical::Center)
    .style(move |_| container::Style {
        background: Some(Background::Color(if selected {
            C_ACCENT
        } else {
            Color::TRANSPARENT
        })),
        border: border::width(1.0)
            .color(if selected { C_ACCENT } else { C_CARD_BORDER })
            .rounded(4.0),
        ..container::Style::default()
    });

    let texts = column![
        text(title.to_string()).size(13).color(C_TEXT_PRIMARY),
        text(if subtitle.is_empty() { " ".to_string() } else { subtitle.to_string() })
            .size(11)
            .color(C_TEXT_SECONDARY),
    ]
    .spacing(2)
    .width(Length::Fill);

    let content = row![check, texts]
        .spacing(10)
        .align_y(alignment::Vertical::Center)
        .padding([6, 10]);

    button(content)
        .width(Length::Fill)
        .padding(0)
        .style(move |_theme, status| candidate_row_style(status, selected))
        .on_press(AppMessage::ForwardTargetToggled(target))
        .into()
}

fn candidate_row_style(status: button::Status, selected: bool) -> button::Style {
    let background = if selected {
        Some(Background::Color(C_ROW_SELECTED))
    } else {
        match status {
            button::Status::Hovered | button::Status::Pressed => {
                Some(Background::Color(C_ROW_HOVER))
            }
            _ => None,
        }
    };
    button::Style {
        background,
        text_color: Color::TRANSPARENT,
        border: border::rounded(6.0),
        shadow: Default::default(),
        snap: true,
    }
}

fn action_row(state: &ForwardPickerState) -> Element<'_, AppMessage> {
    let count = state.selected.len();
    let can_send = count > 0 && !state.submitting;

    let cancel = button(text("取消").size(13).color(C_TEXT_PRIMARY))
        .padding([6, 18])
        .on_press(AppMessage::DismissForwardPicker)
        .style(|_theme, status| {
            let bg = match status {
                button::Status::Hovered | button::Status::Pressed => {
                    Color::from_rgb8(0x3A, 0x41, 0x4B)
                }
                _ => Color::from_rgb8(0x2F, 0x35, 0x3E),
            };
            button::Style {
                background: Some(Background::Color(bg)),
                border: border::width(1.0)
                    .color(Color::from_rgb8(0x4A, 0x52, 0x5E))
                    .rounded(6.0),
                ..button::Style::default()
            }
        });

    let send_label = if state.submitting {
        "发送中...".to_string()
    } else if count == 0 {
        "发送".to_string()
    } else {
        format!("发送 ({})", count)
    };
    let mut send_btn =
        button(text(send_label).size(13).color(Color::from_rgb8(0xFF, 0xFF, 0xFF)))
            .padding([6, 18])
            .style(move |_theme, status| {
                let bg = if !can_send {
                    Color::from_rgb8(0x3A, 0x56, 0x72)
                } else {
                    match status {
                        button::Status::Hovered | button::Status::Pressed => {
                            Color::from_rgb8(0x2A, 0x6C, 0xB2)
                        }
                        _ => C_ACCENT_DIM,
                    }
                };
                button::Style {
                    background: Some(Background::Color(bg)),
                    border: border::rounded(6.0),
                    ..button::Style::default()
                }
            });
    if can_send {
        send_btn = send_btn.on_press(AppMessage::ForwardSendPressed);
    }

    let selected_hint = text(format!("{}/{}", count, FORWARD_MAX_TARGETS))
        .size(11)
        .color(C_TEXT_SECONDARY);

    row![
        selected_hint,
        container(text("")).width(Length::Fill),
        cancel,
        send_btn,
    ]
    .spacing(10)
    .align_y(alignment::Vertical::Center)
    .into()
}
