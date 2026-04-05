use std::collections::HashSet;

use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{alignment, border, Background, Color, Element, Length, Theme};

use crate::app::message::AppMessage;
use crate::app::state::{AddFriendState, SessionListState};
use crate::ui::icons::{self, Icon};

const C_PANEL_BG: Color = Color::from_rgb8(0x22, 0x25, 0x2B);
const C_PANEL_BG_HOVER: Color = Color::from_rgb8(0x2B, 0x30, 0x37);
const C_INPUT_BG: Color = Color::from_rgb8(0x1D, 0x21, 0x27);
const C_INPUT_BORDER: Color = Color::from_rgb8(0x3A, 0x3F, 0x48);
const C_TEXT_PRIMARY: Color = Color::from_rgb8(0xEA, 0xEE, 0xF4);
const C_TEXT_SECONDARY: Color = Color::from_rgb8(0x9E, 0xA5, 0xAE);
const C_ACCENT: Color = Color::from_rgb8(0xC2, 0x76, 0x19);
const C_DIVIDER: Color = Color::from_rgb8(0x35, 0x39, 0x40);

struct NewFriendRequestVm {
    name: &'static str,
    subtitle: &'static str,
    added: bool,
}

/// Render WeChat-like contacts/add-friend list panel (middle column).
pub fn panel_view<'a>(
    state: &'a AddFriendState,
    session_list: &'a SessionListState,
) -> Element<'a, AppMessage> {
    let search_input = text_input("搜索好友", &state.search_input)
        .on_input(|text| AppMessage::AddFriendSearchChanged { text })
        .padding([8, 10])
        .size(14)
        .style(input_style)
        .width(Length::Fill);

    let add_input = text_input("添加好友（用户名 / UID）", &state.add_input)
        .on_input(|text| AppMessage::AddFriendInputChanged { text })
        .on_submit(AppMessage::AddFriendRequestPressed)
        .padding([8, 10])
        .size(14)
        .style(input_style)
        .width(Length::Fill);

    let add_button = button(text("添加").size(13))
        .padding([8, 14])
        .on_press(AppMessage::AddFriendRequestPressed)
        .style(add_button_style);

    let names = friend_names(session_list, &state.search_input);
    let requests = new_friend_requests(&state.search_input);

    let mut list = column![
        panel_card(
            row![
                icons::render(Icon::Search, 15.0, Color::from_rgb8(0x8F, 0x96, 0x9F)),
                search_input
            ]
            .spacing(8)
            .align_y(alignment::Vertical::Center)
        ),
        panel_card(
            row![add_input, add_button]
                .spacing(10)
                .align_y(alignment::Vertical::Center)
        ),
        section_header(
            "新好友消息",
            state.new_friends_expanded,
            Some(requests.len() as u32),
            AppMessage::ToggleNewFriendsSection
        ),
    ]
    .spacing(8)
    .padding([10, 10]);

    if state.new_friends_expanded {
        if requests.is_empty() {
            list = list.push(
                container(
                    text("暂无新的好友申请")
                        .size(13)
                        .color(Color::from_rgb8(0x8D, 0x95, 0x9F)),
                )
                .padding([8, 8]),
            );
        } else {
            for request in requests {
                list = list.push(new_friend_request_item(&request));
            }
        }
    }

    list = list.push(section_header(
        "群列表",
        state.groups_expanded,
        Some(12),
        AppMessage::ToggleGroupSection,
    ));

    if state.groups_expanded {
        list = list
            .push(section_item("项目协作群", Some("18 人"), false))
            .push(section_item("设计讨论组", Some("9 人"), false))
            .push(section_item("Rust 开发群", Some("23 人"), false));
    }

    list = list.push(section_header(
        "好友列表",
        state.friends_expanded,
        Some(names.len() as u32),
        AppMessage::ToggleFriendSection,
    ));

    if state.friends_expanded {
        list = list.push(section_divider());
        if names.is_empty() {
            list = list.push(
                container(
                    text("暂无匹配好友")
                        .size(13)
                        .color(Color::from_rgb8(0x8D, 0x95, 0x9F)),
                )
                .padding([8, 8]),
            );
        } else {
            for name in names.iter().take(120) {
                list = list.push(friend_item(name.clone()));
            }
        }
    }

    if let Some(feedback) = &state.feedback {
        list = list.push(
            container(
                text(feedback)
                    .size(12)
                    .color(Color::from_rgb8(0xD7, 0xC2, 0x9D)),
            )
            .padding([8, 6]),
        );
    }

    container(
        scrollable(list)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(panel_scroll_style),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(|_| container::Style {
        background: Some(Background::Color(C_PANEL_BG)),
        ..container::Style::default()
    })
    .into()
}

/// Render right detail area for add-friend route.
pub fn detail_view() -> Element<'static, AppMessage> {
    container(
        column![
            icons::render(
                Icon::Message,
                52.0,
                Color::from_rgba8(0x8B, 0x92, 0x9B, 0.28)
            ),
            text("请选择一个联系人")
                .size(18)
                .color(Color::from_rgb8(0x7D, 0x84, 0x8E)),
        ]
        .spacing(12)
        .align_x(alignment::Horizontal::Center),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .into()
}

fn panel_card<'a>(content: impl Into<Element<'a, AppMessage>>) -> Element<'a, AppMessage> {
    container(content.into())
        .padding([8, 10])
        .style(|_| container::Style {
            background: Some(Background::Color(C_INPUT_BG)),
            border: border::rounded(8.0).width(1.0).color(C_INPUT_BORDER),
            ..container::Style::default()
        })
        .into()
}

fn section_header(
    title: &str,
    expanded: bool,
    count: Option<u32>,
    on_press: AppMessage,
) -> Element<'_, AppMessage> {
    let arrow = if expanded { "⌄" } else { "›" };

    let count_text = count.map(|value| value.to_string()).unwrap_or_default();

    button(
        container(
            row![
                text(arrow)
                    .size(18)
                    .color(Color::from_rgb8(0x9F, 0xA6, 0xAF)),
                text(title).size(16).color(C_TEXT_PRIMARY),
                container(text(count_text).size(14).color(C_TEXT_SECONDARY))
                    .width(Length::Fill)
                    .align_x(alignment::Horizontal::Right),
            ]
            .spacing(8)
            .align_y(alignment::Vertical::Center),
        )
        .padding([7, 8]),
    )
    .width(Length::Fill)
    .padding(0)
    .style(section_header_button_style)
    .on_press(on_press)
    .into()
}

fn section_item<'a>(
    title: &'a str,
    subtitle: Option<&'a str>,
    highlight: bool,
) -> Element<'a, AppMessage> {
    let subtitle = subtitle.unwrap_or("");
    let fg = if highlight {
        Color::from_rgb8(0xE6, 0xB0, 0x8C)
    } else {
        C_TEXT_SECONDARY
    };

    container(
        row![
            avatar_square(title),
            column![
                text(title).size(16).color(C_TEXT_PRIMARY),
                text(subtitle).size(12).color(fg),
            ]
            .spacing(4)
            .width(Length::Fill),
        ]
        .spacing(10)
        .align_y(alignment::Vertical::Center),
    )
    .padding([7, 12])
    .style(|_| container::Style {
        background: Some(Background::Color(C_PANEL_BG)),
        ..container::Style::default()
    })
    .into()
}

fn new_friend_request_item(request: &NewFriendRequestVm) -> Element<'static, AppMessage> {
    let action: Element<'static, AppMessage> = if request.added {
        text("Added")
            .size(14)
            .color(Color::from_rgb8(0xA3, 0xAA, 0xB3))
            .into()
    } else {
        button(text("查看").size(13))
            .padding([6, 12])
            .style(view_button_style)
            .on_press(AppMessage::Noop)
            .into()
    };

    container(
        row![
            avatar_square(request.name),
            column![
                text(request.name).size(15).color(C_TEXT_PRIMARY),
                text(request.subtitle).size(12).color(C_TEXT_SECONDARY),
            ]
            .spacing(4)
            .width(Length::Fill),
            action,
        ]
        .spacing(10)
        .align_y(alignment::Vertical::Center),
    )
    .padding([7, 12])
    .style(|_| container::Style {
        background: Some(Background::Color(C_PANEL_BG)),
        ..container::Style::default()
    })
    .into()
}

fn friend_item(name: String) -> Element<'static, AppMessage> {
    container(
        row![
            avatar_square(&name),
            text(name).size(16).color(C_TEXT_PRIMARY),
        ]
        .spacing(10)
        .align_y(alignment::Vertical::Center),
    )
    .padding([7, 12])
    .style(|_| container::Style {
        background: Some(Background::Color(C_PANEL_BG)),
        ..container::Style::default()
    })
    .into()
}

fn section_divider() -> Element<'static, AppMessage> {
    container(text(""))
        .height(Length::Fixed(1.0))
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(C_DIVIDER)),
            ..container::Style::default()
        })
        .into()
}

fn avatar_square(label: &str) -> Element<'static, AppMessage> {
    let first = label.chars().next().unwrap_or('友');
    container(
        text(first.to_string())
            .size(12)
            .color(Color::from_rgb8(0xEE, 0xF2, 0xF8)),
    )
    .width(Length::Fixed(34.0))
    .height(Length::Fixed(34.0))
    .align_x(alignment::Horizontal::Center)
    .align_y(alignment::Vertical::Center)
    .style(|_| container::Style {
        background: Some(Background::Color(Color::from_rgb8(0x5A, 0x6F, 0x86))),
        border: border::rounded(6.0),
        ..container::Style::default()
    })
    .into()
}

fn input_style(_theme: &Theme, status: text_input::Status) -> text_input::Style {
    let border_color = match status {
        text_input::Status::Focused { .. } => Color::from_rgb8(0x49, 0x50, 0x5A),
        text_input::Status::Hovered => Color::from_rgb8(0x43, 0x49, 0x53),
        text_input::Status::Active | text_input::Status::Disabled => C_INPUT_BORDER,
    };

    text_input::Style {
        background: Background::Color(C_INPUT_BG),
        border: border::rounded(7.0).width(1.0).color(border_color),
        icon: Color::from_rgb8(0x8F, 0x96, 0x9F),
        placeholder: Color::from_rgb8(0x8F, 0x96, 0x9F),
        value: C_TEXT_PRIMARY,
        selection: Color::from_rgb8(0x6D, 0x4E, 0x23),
    }
}

fn add_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb8(0xD0, 0x82, 0x24),
        button::Status::Pressed => Color::from_rgb8(0xA8, 0x63, 0x14),
        _ => C_ACCENT,
    };

    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::from_rgb8(0xFF, 0xFF, 0xFF),
        border: border::rounded(7.0),
        shadow: Default::default(),
        snap: true,
    }
}

fn section_header_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Some(Background::Color(C_PANEL_BG_HOVER)),
        button::Status::Pressed => Some(Background::Color(Color::from_rgb8(0x30, 0x35, 0x3D))),
        _ => None,
    };

    button::Style {
        background: bg,
        text_color: Color::TRANSPARENT,
        border: border::rounded(7.0),
        shadow: Default::default(),
        snap: true,
    }
}

fn view_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb8(0x35, 0x3A, 0x42),
        button::Status::Pressed => Color::from_rgb8(0x2E, 0x33, 0x3A),
        _ => Color::from_rgb8(0x31, 0x36, 0x3E),
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::from_rgb8(0xE4, 0xE9, 0xF0),
        border: border::rounded(7.0)
            .width(1.0)
            .color(Color::from_rgb8(0x4A, 0x51, 0x5A)),
        shadow: Default::default(),
        snap: true,
    }
}

fn panel_scroll_style(theme: &Theme, status: scrollable::Status) -> scrollable::Style {
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

fn new_friend_requests(query: &str) -> Vec<NewFriendRequestVm> {
    let query = query.trim().to_lowercase();
    let all = vec![
        NewFriendRequestVm {
            name: "小碗要变盆🎸钢琴通",
            subtitle: "我是小碗要变盆🎸钢琴通",
            added: true,
        },
        NewFriendRequestVm {
            name: "不要焦虑 深圳 辅助",
            subtitle: "最近没空呢",
            added: true,
        },
        NewFriendRequestVm {
            name: "张凌静 别踩我脚趾…",
            subtitle: "Me: 打游戏",
            added: true,
        },
        NewFriendRequestVm {
            name: "Camellia 迪迪很哆…",
            subtitle: "我是 Camellia",
            added: true,
        },
        NewFriendRequestVm {
            name: "新的朋友申请 A",
            subtitle: "来自手机通讯录",
            added: false,
        },
        NewFriendRequestVm {
            name: "新的朋友申请 B",
            subtitle: "通过群聊添加",
            added: false,
        },
    ];

    if query.is_empty() {
        return all;
    }

    all.into_iter()
        .filter(|item| {
            item.name.to_lowercase().contains(&query) || item.subtitle.to_lowercase().contains(&query)
        })
        .collect()
}

fn friend_names(session_list: &SessionListState, query: &str) -> Vec<String> {
    let query = query.trim().to_lowercase();
    let mut seen = HashSet::new();
    let mut names = Vec::new();

    for item in &session_list.items {
        let title = item.title.trim();
        if title.is_empty() {
            continue;
        }

        if !query.is_empty() && !title.to_lowercase().contains(&query) {
            continue;
        }

        if seen.insert(title.to_string()) {
            names.push(title.to_string());
        }
    }

    if names.is_empty() {
        let fallback = [
            "李欣蕊",
            "游哥",
            "Jolin",
            "Jenny",
            "威廉",
            "玫瑰",
            "小王",
            "阿峰",
        ];
        for name in fallback {
            if !query.is_empty() && !name.to_lowercase().contains(&query) {
                continue;
            }
            names.push(name.to_string());
        }
    }

    names
}
