use iced::widget::{column, container, mouse_area, row, stack, text};
use iced::{alignment, border, mouse, Background, Color, Element, Length};

use crate::app::message::AppMessage;
use crate::app::route::Route;
use crate::app::state::AppState;
use crate::ui::icons::{self, Icon};
use crate::ui::screens::{chat, session_list, settings};

const SIDEBAR_WIDTH: f32 = 70.0;
const C_ROOT_BG: Color = Color::from_rgb8(0x1F, 0x23, 0x29);
const C_SIDEBAR_BG: Color = Color::from_rgb8(0x22, 0x29, 0x31);
const C_LIST_BG: Color = Color::from_rgb8(0x2A, 0x2D, 0x33);
const C_CHAT_BG: Color = Color::from_rgb8(0x18, 0x1A, 0x1F);
const C_DIVIDER: Color = Color::from_rgb8(0x35, 0x39, 0x40);

/// Render authenticated desktop workspace with WeChat-like 3-column layout.
pub fn view(state: &AppState) -> Element<'_, AppMessage> {
    let active_chat = state
        .active_chat
        .as_ref()
        .map(|chat_state| (chat_state.channel_id, chat_state.channel_type));

    let active_title = state.active_chat.as_ref().and_then(|active| {
        state
            .session_list
            .items
            .iter()
            .find(|item| {
                item.channel_id == active.channel_id && item.channel_type == active.channel_type
            })
            .map(|item| item.title.as_str())
    });

    let detail: Element<'_, AppMessage> = match state.route {
        Route::Chat => {
            if let Some(chat_state) = &state.active_chat {
                chat::view(chat_state, active_title.unwrap_or("会话"))
            } else {
                empty_detail("请选择一个会话")
            }
        }
        Route::Settings => settings::view(&state.settings),
        Route::SessionList => empty_detail("请选择一个会话"),
        Route::Login => empty_detail(""),
        Route::Splash => empty_detail(""),
    };

    container(
        row![
            sidebar(state),
            divider(),
            container(session_list::view(&state.session_list, active_chat))
                .width(Length::Fixed(state.layout.session_list_width))
                .height(Length::Fill)
                .style(|_| panel(C_LIST_BG)),
            session_splitter(),
            container(detail)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_| panel(C_CHAT_BG)),
        ]
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(|_| panel(C_ROOT_BG))
    .into()
}

fn sidebar(state: &AppState) -> Element<'_, AppMessage> {
    let top = column![
        avatar_chip(state.auth.user_id),
        nav_icon(Icon::Message, true, Some(2)),
        nav_icon(Icon::Contact, false, None),
        nav_icon(Icon::Box, false, None),
        nav_icon(Icon::Compass, false, None),
        nav_icon(Icon::Link, false, None),
    ]
    .spacing(12)
    .align_x(alignment::Horizontal::Center);

    let bottom = column![nav_icon(Icon::Settings, false, None),]
        .spacing(12)
        .align_x(alignment::Horizontal::Center);

    container(
        column![top, container(text("")).height(Length::Fill), bottom,]
            .padding([12, 6])
            .height(Length::Fill)
            .align_x(alignment::Horizontal::Center),
    )
    .width(Length::Fixed(SIDEBAR_WIDTH))
    .height(Length::Fill)
    .style(|_| panel(C_SIDEBAR_BG))
    .into()
}

fn avatar_chip(user_id: Option<u64>) -> Element<'static, AppMessage> {
    let label = user_id
        .map(|id| format!("{:02}", id % 100))
        .unwrap_or_else(|| "ME".to_string());
    container(
        text(label)
            .size(12)
            .color(Color::from_rgb8(0xE9, 0xEE, 0xF5)),
    )
    .width(Length::Fixed(42.0))
    .height(Length::Fixed(42.0))
    .align_x(alignment::Horizontal::Center)
    .align_y(alignment::Vertical::Center)
    .style(|_| container::Style {
        background: Some(Background::Color(Color::from_rgb8(0x3F, 0x4A, 0x58))),
        border: border::rounded(6.0),
        ..container::Style::default()
    })
    .into()
}

fn nav_icon(icon: Icon, active: bool, badge_count: Option<u32>) -> Element<'static, AppMessage> {
    let icon_color = if active {
        Color::from_rgb8(0xF3, 0xF6, 0xF8)
    } else {
        Color::from_rgb8(0xA6, 0xAE, 0xB8)
    };

    let chip = container(icons::render(icon, 27.0, icon_color))
        .width(Length::Fixed(46.0))
        .height(Length::Fixed(42.0))
        .align_x(alignment::Horizontal::Center)
        .align_y(alignment::Vertical::Center)
        .style(move |_| container::Style {
            background: if active {
                Some(Background::Color(Color::from_rgb8(0xC2, 0x76, 0x19)))
            } else {
                None
            },
            border: border::rounded(12.0),
            ..container::Style::default()
        });

    if let Some(count) = badge_count {
        let badge = container(
            text(count.to_string())
                .size(10)
                .color(Color::from_rgb8(0xFF, 0xFF, 0xFF)),
        )
        .width(Length::Fixed(19.0))
        .height(Length::Fixed(19.0))
        .align_x(alignment::Horizontal::Center)
        .align_y(alignment::Vertical::Center)
        .style(|_| container::Style {
            background: Some(Background::Color(Color::from_rgb8(0xEA, 0x4B, 0x52))),
            border: border::rounded(9.5),
            ..container::Style::default()
        });

        let layered = stack![
            container(chip)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(alignment::Horizontal::Center)
                .align_y(alignment::Vertical::Center),
            container(
                row![container(text("")).width(Length::Fill), badge]
                    .width(Length::Fill)
                    .align_y(alignment::Vertical::Top)
            )
            .width(Length::Fill)
            .height(Length::Fill)
        ]
        .width(Length::Fixed(52.0))
        .height(Length::Fixed(44.0));

        return container(layered)
            .width(Length::Fill)
            .align_x(alignment::Horizontal::Center)
            .into();
    }

    container(chip)
        .width(Length::Fill)
        .align_x(alignment::Horizontal::Center)
        .into()
}

fn divider() -> Element<'static, AppMessage> {
    container(text(""))
        .width(Length::Fixed(1.0))
        .height(Length::Fill)
        .style(|_| panel(C_DIVIDER))
        .into()
}

fn session_splitter() -> Element<'static, AppMessage> {
    mouse_area(
        container(text(""))
            .width(Length::Fixed(2.0))
            .height(Length::Fill),
    )
    .interaction(mouse::Interaction::ResizingHorizontally)
    .on_press(AppMessage::SessionSplitterDragStarted)
    .on_release(AppMessage::SessionSplitterDragEnded)
    .into()
}

fn empty_detail(text_value: &str) -> Element<'_, AppMessage> {
    container(
        text(text_value)
            .size(22)
            .color(Color::from_rgb8(0x8A, 0x90, 0x99)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .into()
}

fn panel(background: Color) -> container::Style {
    container::Style {
        background: Some(Background::Color(background)),
        ..container::Style::default()
    }
}
