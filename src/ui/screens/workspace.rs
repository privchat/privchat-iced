use iced::widget::{button, column, container, mouse_area, row, stack, text};
use iced::{alignment, border, mouse, Background, Color, Element, Length};

use crate::app::message::AppMessage;
use crate::app::route::Route;
use crate::app::state::AppState;
use crate::ui::icons::{self, Icon};
use crate::ui::screens::{add_friend, chat, session_list, settings};
use crate::ui::widgets::{forward_picker, friend_settings, group_settings};

const SIDEBAR_WIDTH: f32 = 70.0;
const C_ROOT_BG: Color = Color::from_rgb8(0x1F, 0x23, 0x29);
const C_SIDEBAR_BG: Color = Color::from_rgb8(0x22, 0x29, 0x31);
const C_LIST_BG: Color = Color::from_rgb8(0x2A, 0x2D, 0x33);
const C_CHAT_BG: Color = Color::from_rgb8(0x18, 0x1A, 0x1F);
const C_DIVIDER: Color = Color::from_rgb8(0x35, 0x39, 0x40);
const SETTINGS_MENU_WIDTH: f32 = 132.0;
const SETTINGS_MENU_BOTTOM_GAP: f32 = 56.0;
const SETTINGS_MENU_LEFT_OFFSET: f32 = SIDEBAR_WIDTH + 8.0;

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
    let active_title = active_title
        .or_else(|| state.active_chat.as_ref().map(|chat| chat.title.as_str()))
        .unwrap_or("PrivChat");

    let detail: Element<'_, AppMessage> = match state.route {
        Route::Chat => {
            if let Some(chat_state) = &state.active_chat {
                let active_presence = chat_state
                    .peer_user_id
                    .and_then(|user_id| state.presences.get(&user_id));
                chat::view(
                    chat_state,
                    active_title,
                    active_presence,
                    chat_state.typing_hint.as_deref(),
                    &state.image_cache,
                    state.voice_playback.as_ref().map(|h| h.message_id),
                )
            } else {
                empty_detail("请选择一个会话")
            }
        }
        Route::AddFriend => add_friend::detail_view(&state.add_friend),
        Route::Settings => settings::view(&state.settings),
        Route::SessionList => empty_detail("请选择一个会话"),
        Route::SwitchAccount => empty_detail(""),
        Route::Login => empty_detail(""),
        Route::Splash => empty_detail(""),
    };

    let middle_panel: Element<'_, AppMessage> = match state.route {
        Route::AddFriend => add_friend::panel_view(state),
        _ => session_list::view(state, active_chat, state.layout.session_list_width),
    };

    let workspace = container(
        row![
            sidebar(state),
            divider(),
            container(middle_panel)
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
    .style(|_| panel(C_ROOT_BG));

    let composed: Element<'_, AppMessage> = if state.overlay.settings_menu_open {
        stack![
            workspace,
            mouse_area(container(text("")).width(Length::Fill).height(Length::Fill))
                .on_press(AppMessage::DismissSettingsMenu),
            settings_menu_layer(),
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else {
        workspace.into()
    };

    let composed: Element<'_, AppMessage> = if let Some(picker) = state.forward_picker.as_ref() {
        stack![
            composed,
            mouse_area(
                container(text(""))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(|_| container::Style {
                        background: Some(Background::Color(Color::from_rgba8(0, 0, 0, 0.5))),
                        ..container::Style::default()
                    })
            )
            .on_press(AppMessage::DismissForwardPicker),
            container(forward_picker::view(picker))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(alignment::Horizontal::Center)
                .align_y(alignment::Vertical::Center),
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else {
        composed
    };

    let composed: Element<'_, AppMessage> = if let Some(panel) = state.friend_settings.as_ref() {
        stack![
            composed,
            mouse_area(
                container(text(""))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(|_| container::Style {
                        background: Some(Background::Color(Color::from_rgba8(0, 0, 0, 0.5))),
                        ..container::Style::default()
                    })
            )
            .on_press(AppMessage::DismissFriendSettings),
            container(friend_settings::view(panel))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(alignment::Horizontal::Center)
                .align_y(alignment::Vertical::Center),
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else {
        composed
    };

    if let Some(panel) = state.group_settings.as_ref() {
        stack![
            composed,
            mouse_area(
                container(text(""))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(|_| container::Style {
                        background: Some(Background::Color(Color::from_rgba8(0, 0, 0, 0.5))),
                        ..container::Style::default()
                    })
            )
            .on_press(AppMessage::DismissGroupSettings),
            container(group_settings::view(panel))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(alignment::Horizontal::Center)
                .align_y(alignment::Vertical::Center),
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else {
        composed
    }
}

fn sidebar(state: &AppState) -> Element<'_, AppMessage> {
    let message_badge_count = (state.session_list.total_unread_count > 0)
        .then_some(state.session_list.total_unread_count);
    let add_friend_badge_count = state
        .add_friend
        .requests
        .iter()
        .filter(|item| !item.is_added)
        .count() as u32;
    let add_friend_badge_count = (add_friend_badge_count > 0).then_some(add_friend_badge_count);
    let message_active = matches!(state.route, Route::SessionList | Route::Chat);
    let add_friend_active = matches!(state.route, Route::AddFriend);
    let settings_active =
        matches!(state.route, Route::Settings) || state.overlay.settings_menu_open;

    let top = column![
        avatar_chip(&state.auth.username),
        nav_icon(
            Icon::Message,
            message_active,
            message_badge_count,
            AppMessage::OpenSessionListPage
        ),
        nav_icon(
            Icon::Contact,
            add_friend_active,
            add_friend_badge_count,
            AppMessage::OpenAddFriendPage
        ),
    ]
    .spacing(12)
    .align_x(alignment::Horizontal::Center);

    let bottom = column![nav_icon(
        Icon::Settings,
        settings_active,
        None,
        AppMessage::ToggleSettingsMenu
    )]
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

fn avatar_chip(username: &str) -> Element<'static, AppMessage> {
    let label = username
        .trim()
        .chars()
        .next()
        .map(|ch| ch.to_uppercase().to_string())
        .unwrap_or_else(|| "M".to_string());
    container(
        text(label)
            .size(18)
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

fn nav_icon(
    icon: Icon,
    active: bool,
    badge_count: Option<u32>,
    on_press: AppMessage,
) -> Element<'static, AppMessage> {
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

    let content: Element<'static, AppMessage> = if let Some(count) = badge_count {
        let label = if count > 99 {
            "99+".to_string()
        } else {
            count.to_string()
        };
        let badge = container(
            text(label)
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

        container(layered)
            .width(Length::Fill)
            .align_x(alignment::Horizontal::Center)
            .into()
    } else {
        container(chip)
            .width(Length::Fill)
            .align_x(alignment::Horizontal::Center)
            .into()
    };

    button(content)
        .width(Length::Fill)
        .padding(0)
        .style(nav_button_style)
        .on_press(on_press)
        .into()
}

fn nav_button_style(_theme: &iced::Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered => {
            Some(Background::Color(Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.03)))
        }
        button::Status::Pressed => {
            Some(Background::Color(Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.06)))
        }
        _ => None,
    };
    button::Style {
        background,
        text_color: Color::TRANSPARENT,
        border: border::width(0.0),
        shadow: Default::default(),
        snap: true,
    }
}

fn settings_menu_layer() -> Element<'static, AppMessage> {
    container(
        column![
            container(text("")).height(Length::Fill),
            row![
                container(text("")).width(Length::Fixed(SETTINGS_MENU_LEFT_OFFSET)),
                settings_menu_popup(),
                container(text("")).width(Length::Fill),
            ]
            .height(Length::Shrink),
            container(text("")).height(Length::Fixed(SETTINGS_MENU_BOTTOM_GAP)),
        ]
        .width(Length::Fill)
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn settings_menu_popup() -> Element<'static, AppMessage> {
    container(
        column![
            button(
                container(
                    text("设置")
                        .size(14)
                        .color(Color::from_rgb8(0xE6, 0xEB, 0xF2))
                )
                .width(Length::Fill)
                .padding([8, 12]),
            )
            .width(Length::Fill)
            .style(menu_item_style)
            .on_press(AppMessage::SettingsMenuOpenSettings),
            button(
                container(
                    text("切换账号")
                        .size(14)
                        .color(Color::from_rgb8(0xE6, 0xEB, 0xF2))
                )
                .width(Length::Fill)
                .padding([8, 12]),
            )
            .width(Length::Fill)
            .style(menu_item_style)
            .on_press(AppMessage::SettingsMenuSwitchAccount),
            button(
                container(
                    text("日志窗口")
                        .size(14)
                        .color(Color::from_rgb8(0xE6, 0xEB, 0xF2))
                )
                .width(Length::Fill)
                .padding([8, 12]),
            )
            .width(Length::Fill)
            .style(menu_item_style)
            .on_press(AppMessage::SettingsMenuOpenLogs),
            button(
                container(
                    text("退出")
                        .size(14)
                        .color(Color::from_rgb8(0xE6, 0xB0, 0x8C))
                )
                .width(Length::Fill)
                .padding([8, 12]),
            )
            .width(Length::Fill)
            .style(menu_item_style)
            .on_press(AppMessage::SettingsMenuLogout),
        ]
        .spacing(2),
    )
    .width(Length::Fixed(SETTINGS_MENU_WIDTH))
    .style(|_| container::Style {
        background: Some(Background::Color(Color::from_rgb8(0x2C, 0x30, 0x37))),
        border: border::rounded(8.0)
            .width(1.0)
            .color(Color::from_rgb8(0x3B, 0x41, 0x4A)),
        ..container::Style::default()
    })
    .into()
}

fn menu_item_style(_theme: &iced::Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered => Some(Background::Color(Color::from_rgb8(0x39, 0x3E, 0x47))),
        button::Status::Pressed => Some(Background::Color(Color::from_rgb8(0x43, 0x48, 0x52))),
        _ => None,
    };

    button::Style {
        background,
        text_color: Color::TRANSPARENT,
        border: border::rounded(6.0),
        shadow: Default::default(),
        snap: true,
    }
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
