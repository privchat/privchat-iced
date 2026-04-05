use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{alignment, border, Background, Color, Element, Length, Theme};

use crate::app::message::AppMessage;
use crate::app::state::AddFriendState;
use crate::presentation::vm::{
    AddFriendSelectionVm, FriendListItemVm, FriendRequestItemVm, GroupListItemVm, SearchUserVm,
};
use crate::ui::icons::{self, Icon};

const C_PANEL_BG: Color = Color::from_rgb8(0x2B, 0x2E, 0x34);
const C_PANEL_BG_HOVER: Color = Color::from_rgb8(0x2E, 0x33, 0x3A);
const C_PANEL_BG_SELECTED: Color = Color::from_rgb8(0x4C, 0x50, 0x57);
const C_SEARCH_BG: Color = Color::from_rgb8(0x24, 0x27, 0x2D);
const C_SEARCH_BORDER: Color = Color::from_rgb8(0x3A, 0x3F, 0x47);
const C_TEXT_PRIMARY: Color = Color::from_rgb8(0xEA, 0xEE, 0xF4);
const C_TEXT_SECONDARY: Color = Color::from_rgb8(0x9E, 0xA5, 0xAE);
const C_DIVIDER: Color = Color::from_rgb8(0x35, 0x39, 0x40);
const C_POPUP_BG: Color = Color::from_rgb8(0x24, 0x26, 0x2C);
const C_POPUP_CARD_BG: Color = Color::from_rgb8(0x2D, 0x31, 0x38);
const C_POPUP_SUCCESS: Color = Color::from_rgb8(0x1D, 0xC4, 0x72);

pub fn panel_view<'a>(state: &'a AddFriendState) -> Element<'a, AppMessage> {
    let query = state.search_input.trim().to_lowercase();

    let requests = state
        .requests
        .iter()
        .filter(|item| matches_query(&query, &item.title, &item.subtitle))
        .collect::<Vec<_>>();
    let groups = state
        .groups
        .iter()
        .filter(|item| matches_query(&query, &item.title, &item.subtitle))
        .collect::<Vec<_>>();
    let friends = state
        .friends
        .iter()
        .filter(|item| matches_query(&query, &item.title, &item.subtitle))
        .collect::<Vec<_>>();

    let mut list = column![];
    if let Some(error) = &state.contacts_error {
        list = list.push(
            container(
                text(error)
                    .size(12)
                    .color(Color::from_rgb8(0xD8, 0x89, 0x89)),
            )
            .padding([8, 10]),
        );
    }

    list = list.push(section_header(
        "新好友消息",
        state.new_friends_expanded,
        Some(requests.len() as u32),
        AppMessage::ToggleNewFriendsSection,
    ));

    if state.new_friends_expanded {
        if requests.is_empty() {
            list = list.push(empty_tip("暂无新的好友申请"));
        } else {
            for item in requests {
                let selected = state.selected_panel_item
                    == Some(AddFriendSelectionVm::Request(item.from_user_id));
                list = list.push(friend_request_item(item, selected));
            }
        }
    }

    list = list.push(section_header(
        "群列表",
        state.groups_expanded,
        Some(groups.len() as u32),
        AppMessage::ToggleGroupSection,
    ));

    if state.groups_expanded {
        if groups.is_empty() {
            list = list.push(empty_tip("暂无群组"));
        } else {
            for item in groups {
                let selected =
                    state.selected_panel_item == Some(AddFriendSelectionVm::Group(item.group_id));
                list = list.push(group_item(item, selected));
            }
        }
    }

    list = list.push(section_header(
        "好友列表",
        state.friends_expanded,
        Some(friends.len() as u32),
        AppMessage::ToggleFriendSection,
    ));

    if state.friends_expanded {
        list = list.push(section_divider());
        if friends.is_empty() {
            list = list.push(empty_tip("暂无匹配好友"));
        } else {
            for item in friends {
                let selected =
                    state.selected_panel_item == Some(AddFriendSelectionVm::Friend(item.user_id));
                list = list.push(friend_item(item, selected));
            }
        }
    }

    column![
        search_bar(&state.search_input),
        scrollable(list.spacing(8).padding([0, 10]))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(panel_scroll_style),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

pub fn detail_view<'a>(state: &'a AddFriendState) -> Element<'a, AppMessage> {
    if state.detail_loading {
        return center_tip("资料加载中...");
    }

    if let Some(error) = &state.detail_error {
        return center_tip(error);
    }

    if let Some(detail) = &state.detail {
        let mut fields = column!().spacing(10);
        for item in &detail.fields {
            fields = fields.push(
                row![
                    container(text(&item.label).size(13).color(C_TEXT_SECONDARY))
                        .width(Length::Fixed(110.0)),
                    text(&item.value)
                        .size(14)
                        .wrapping(iced::widget::text::Wrapping::Word)
                        .color(C_TEXT_PRIMARY),
                ]
                .spacing(12),
            );
        }

        let mut content = column![text(&detail.title)
            .size(34)
            .color(C_TEXT_PRIMARY)
            .wrapping(iced::widget::text::Wrapping::Word),]
        .spacing(16);

        if !detail.subtitle.trim().is_empty() {
            content = content.push(
                text(&detail.subtitle)
                    .size(17)
                    .color(C_TEXT_SECONDARY)
                    .wrapping(iced::widget::text::Wrapping::Word),
            );
        }

        content = content
            .push(section_divider())
            .push(fields)
            .width(Length::Fill)
            .height(Length::Shrink);

        return container(
            scrollable(
                container(content)
                    .width(Length::Fill)
                    .style(|_| container::Style {
                        background: Some(Background::Color(Color::from_rgb8(0x1D, 0x20, 0x26))),
                        border: border::rounded(12.0)
                            .width(1.0)
                            .color(Color::from_rgb8(0x2F, 0x35, 0x3D)),
                        ..container::Style::default()
                    })
                    .padding([24, 26]),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .style(detail_scroll_style),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding([24, 24])
        .into();
    }

    center_tip("请选择一个联系人")
}

fn empty_tip(label: &str) -> Element<'_, AppMessage> {
    container(
        text(label)
            .size(13)
            .color(Color::from_rgb8(0x8D, 0x95, 0x9F)),
    )
    .padding([8, 8])
    .into()
}

fn center_tip(label: &str) -> Element<'_, AppMessage> {
    container(
        column![
            icons::render(
                Icon::Message,
                52.0,
                Color::from_rgba8(0x8B, 0x92, 0x9B, 0.28)
            ),
            text(label)
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

fn matches_query(query: &str, title: &str, subtitle: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let title = title.to_lowercase();
    let subtitle = subtitle.to_lowercase();
    title.contains(query) || subtitle.contains(query)
}

fn search_bar(search_value: &str) -> Element<'_, AppMessage> {
    let search_input = text_input("Search", search_value)
        .on_input(|text| AppMessage::AddFriendSearchChanged { text })
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
    .padding([0, 10])
    .width(Length::Fill);

    let plus = button(icons::render(
        Icon::Plus,
        21.0,
        Color::from_rgb8(0x9E, 0xA6, 0xAF),
    ))
    .padding([8, 8])
    .on_press(AppMessage::OpenAddFriendSearchWindow)
    .style(plus_button_style);

    container(row![input_with_icon, plus].spacing(10).width(Length::Fill))
        .padding([10, 12])
        .style(|_| container::Style {
            background: Some(Background::Color(C_PANEL_BG)),
            ..container::Style::default()
        })
        .into()
}

pub fn search_window_view(state: &AddFriendState) -> Element<'_, AppMessage> {
    let header = row![
        container(text("")).width(Length::Fill),
        text("Add Contacts")
            .size(22)
            .color(Color::from_rgb8(0xEA, 0xEE, 0xF4)),
        container(
            button(text("×").size(20).color(Color::from_rgb8(0xC9, 0xCF, 0xD8)))
                .padding([2, 10])
                .style(popup_close_button_style)
                .on_press(AppMessage::CloseAddFriendSearchWindow)
        )
        .width(Length::Fill)
        .align_x(alignment::Horizontal::Right),
    ]
    .align_y(alignment::Vertical::Center);

    let search_input = text_input("搜索用户 / UID", &state.add_input)
        .on_input(|text| AppMessage::AddFriendInputChanged { text })
        .on_submit(AppMessage::AddFriendSearchPressed)
        .padding([10, 12])
        .size(17)
        .style(popup_search_input_style)
        .width(Length::Fill);

    let search_row = row![
        container(
            row![
                icons::render(Icon::Search, 22.0, Color::from_rgb8(0x99, 0xA1, 0xAB)),
                search_input,
                button(text("×").size(20).color(Color::from_rgb8(0xB8, 0xBF, 0xC8)))
                    .padding([2, 8])
                    .style(popup_clear_button_style)
                    .on_press(AppMessage::AddFriendInputChanged {
                        text: String::new()
                    }),
            ]
            .spacing(10)
            .align_y(alignment::Vertical::Center),
        )
        .padding([0, 8])
        .style(|_| container::Style {
            background: Some(Background::Color(C_SEARCH_BG)),
            border: border::rounded(10.0),
            ..container::Style::default()
        })
        .width(Length::Fill),
        button(
            text("Search")
                .size(17)
                .color(Color::from_rgb8(0xFF, 0xFF, 0xFF))
        )
        .padding([10, 18])
        .style(popup_search_button_style)
        .on_press(AppMessage::AddFriendSearchPressed),
    ]
    .spacing(10)
    .align_y(alignment::Vertical::Center);

    let mut body = column![header, search_row].spacing(14);

    if state.search_loading {
        body = body.push(
            container(
                text("Searching...")
                    .size(15)
                    .color(Color::from_rgb8(0x9F, 0xA6, 0xAF)),
            )
            .padding([12, 8]),
        );
    } else if let Some(error) = &state.search_error {
        body = body.push(
            container(
                text(error)
                    .size(14)
                    .color(Color::from_rgb8(0xDD, 0x93, 0x93)),
            )
            .padding([8, 8]),
        );
    } else if !state.search_results.is_empty() {
        let mut quick_list = column!().spacing(6);
        for user in &state.search_results {
            let selected = state.selected_search_user_id == Some(user.user_id);
            quick_list = quick_list.push(search_result_tile(user, selected));
        }
        body = body.push(container(quick_list).padding([6, 4]).style(|_| {
            container::Style {
                background: Some(Background::Color(Color::from_rgb8(0x27, 0x2B, 0x31))),
                border: border::rounded(10.0)
                    .width(1.0)
                    .color(Color::from_rgb8(0x39, 0x3F, 0x48)),
                ..container::Style::default()
            }
        }));

        if let Some(selected_user) = state
            .selected_search_user_id
            .and_then(|id| state.search_results.iter().find(|user| user.user_id == id))
            .or_else(|| state.search_results.first())
        {
            body = body.push(search_result_card(
                selected_user,
                true,
                state.feedback.as_deref(),
            ));
        }
    }

    container(body)
        .padding([18, 18])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(C_POPUP_BG)),
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
            .align_y(alignment::Vertical::Center)
            .width(Length::Fill),
        )
        .padding([7, 8]),
    )
    .width(Length::Fill)
    .padding(0)
    .style(section_header_button_style)
    .on_press(on_press)
    .into()
}

fn friend_request_item(item: &FriendRequestItemVm, selected: bool) -> Element<'static, AppMessage> {
    let title = truncate_single_line(&item.title, 22);
    let subtitle = truncate_single_line(&item.subtitle, 30);
    let selection = AddFriendSelectionVm::Request(item.from_user_id);

    let action: Element<'static, AppMessage> = if item.is_added {
        text("Added")
            .size(14)
            .color(Color::from_rgb8(0xA3, 0xAA, 0xB3))
            .into()
    } else {
        container(
            text("查看")
                .size(13)
                .color(Color::from_rgb8(0xE6, 0xEA, 0xF0)),
        )
        .padding([6, 12])
        .style(|_| container::Style {
            background: Some(Background::Color(Color::from_rgb8(0x31, 0x36, 0x3E))),
            border: border::rounded(7.0)
                .width(1.0)
                .color(Color::from_rgb8(0x4A, 0x51, 0x5A)),
            ..container::Style::default()
        })
        .into()
    };

    button(
        container(
            row![
                avatar_square(&item.title),
                column![
                    text(title)
                        .size(15)
                        .wrapping(iced::widget::text::Wrapping::None)
                        .color(C_TEXT_PRIMARY),
                    text(subtitle)
                        .size(12)
                        .wrapping(iced::widget::text::Wrapping::None)
                        .color(C_TEXT_SECONDARY),
                ]
                .spacing(4)
                .width(Length::Fill),
                container(action)
                    .width(Length::Fixed(56.0))
                    .align_x(alignment::Horizontal::Right),
            ]
            .spacing(10)
            .align_y(alignment::Vertical::Center)
            .width(Length::Fill),
        )
        .width(Length::Fill)
        .padding([7, 12]),
    )
    .width(Length::Fill)
    .padding(0)
    .style(move |_theme, status| list_item_style(selected, status))
    .on_press(AppMessage::AddFriendPanelSelected { item: selection })
    .into()
}

fn group_item(item: &GroupListItemVm, selected: bool) -> Element<'static, AppMessage> {
    let title = truncate_single_line(&item.title, 24);
    let subtitle = truncate_single_line(&item.subtitle, 30);
    let selection = AddFriendSelectionVm::Group(item.group_id);

    button(
        container(
            row![
                avatar_square(&item.title),
                column![
                    text(title)
                        .size(16)
                        .wrapping(iced::widget::text::Wrapping::None)
                        .color(C_TEXT_PRIMARY),
                    text(subtitle)
                        .size(12)
                        .wrapping(iced::widget::text::Wrapping::None)
                        .color(C_TEXT_SECONDARY),
                ]
                .spacing(4)
                .width(Length::Fill),
            ]
            .spacing(10)
            .align_y(alignment::Vertical::Center)
            .width(Length::Fill),
        )
        .width(Length::Fill)
        .padding([7, 12]),
    )
    .width(Length::Fill)
    .padding(0)
    .style(move |_theme, status| list_item_style(selected, status))
    .on_press(AppMessage::AddFriendPanelSelected { item: selection })
    .into()
}

fn friend_item(item: &FriendListItemVm, selected: bool) -> Element<'static, AppMessage> {
    let title = truncate_single_line(&item.title, 24);
    let subtitle = truncate_single_line(&item.subtitle, 30);
    let selection = AddFriendSelectionVm::Friend(item.user_id);

    button(
        container(
            row![
                avatar_square(&item.title),
                column![
                    text(title)
                        .size(16)
                        .wrapping(iced::widget::text::Wrapping::None)
                        .color(C_TEXT_PRIMARY),
                    text(subtitle)
                        .size(12)
                        .wrapping(iced::widget::text::Wrapping::None)
                        .color(C_TEXT_SECONDARY),
                ]
                .spacing(4)
                .width(Length::Fill),
            ]
            .spacing(10)
            .align_y(alignment::Vertical::Center)
            .width(Length::Fill),
        )
        .width(Length::Fill)
        .padding([7, 12]),
    )
    .width(Length::Fill)
    .padding(0)
    .style(move |_theme, status| list_item_style(selected, status))
    .on_press(AppMessage::AddFriendPanelSelected { item: selection })
    .into()
}

fn list_item_style(selected: bool, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered => Some(Background::Color(C_PANEL_BG_HOVER)),
        button::Status::Pressed => Some(Background::Color(Color::from_rgb8(0x30, 0x35, 0x3D))),
        _ => {
            if selected {
                Some(Background::Color(C_PANEL_BG_SELECTED))
            } else {
                Some(Background::Color(C_PANEL_BG))
            }
        }
    };

    button::Style {
        background,
        text_color: Color::TRANSPARENT,
        border: border::rounded(7.0),
        shadow: Default::default(),
        snap: true,
    }
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

fn popup_search_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb8(0x19, 0xCF, 0x78),
        button::Status::Pressed => Color::from_rgb8(0x17, 0xA8, 0x63),
        _ => C_POPUP_SUCCESS,
    };

    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::from_rgb8(0xFF, 0xFF, 0xFF),
        border: border::rounded(10.0),
        shadow: Default::default(),
        snap: true,
    }
}

fn popup_close_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => {
            Some(Background::Color(Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.08)))
        }
        button::Status::Pressed => {
            Some(Background::Color(Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.14)))
        }
        _ => None,
    };

    button::Style {
        background: bg,
        text_color: Color::from_rgb8(0xD6, 0xDE, 0xE8),
        border: border::rounded(8.0),
        shadow: Default::default(),
        snap: true,
    }
}

fn popup_clear_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => {
            Some(Background::Color(Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.08)))
        }
        button::Status::Pressed => {
            Some(Background::Color(Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.12)))
        }
        _ => None,
    };

    button::Style {
        background: bg,
        text_color: Color::from_rgb8(0xBE, 0xC5, 0xCE),
        border: border::rounded(16.0),
        shadow: Default::default(),
        snap: true,
    }
}

fn popup_search_input_style(_theme: &Theme, status: text_input::Status) -> text_input::Style {
    let border_color = match status {
        text_input::Status::Focused { .. } => Color::from_rgb8(0x42, 0x4A, 0x54),
        text_input::Status::Hovered => Color::from_rgb8(0x3B, 0x42, 0x4B),
        text_input::Status::Active | text_input::Status::Disabled => C_SEARCH_BORDER,
    };

    text_input::Style {
        background: Background::Color(C_SEARCH_BG),
        border: border::width(0.0).rounded(10.0).color(border_color),
        icon: Color::from_rgb8(0x8F, 0x96, 0x9F),
        placeholder: Color::from_rgb8(0x8F, 0x96, 0x9F),
        value: Color::from_rgb8(0xE2, 0xE7, 0xEE),
        selection: Color::from_rgb8(0x47, 0x8F, 0x67),
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

fn panel_scroll_style(theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let mut style = scrollable::default(theme, status);
    style.container = container::Style {
        background: Some(Background::Color(C_PANEL_BG)),
        ..container::Style::default()
    };
    style.vertical_rail.background = None;
    style.vertical_rail.border = border::width(0.0).rounded(0.0).color(Color::TRANSPARENT);
    style.vertical_rail.scroller.background = Background::Color(Color::from_rgba8(0, 0, 0, 0.0));
    style.vertical_rail.scroller.border = border::width(0.0)
        .rounded(0.0)
        .color(Color::from_rgba8(0, 0, 0, 0.0));
    style
}

fn detail_scroll_style(theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let mut style = scrollable::default(theme, status);
    style.container = container::Style {
        background: None,
        ..container::Style::default()
    };
    style.vertical_rail.background = Some(Background::Color(Color::from_rgba8(0, 0, 0, 0.0)));
    style.vertical_rail.border = border::width(0.0).rounded(0.0).color(Color::TRANSPARENT);
    style.vertical_rail.scroller.background = Background::Color(Color::from_rgb8(0x4A, 0x50, 0x58));
    style.vertical_rail.scroller.border = border::rounded(6.0);
    style
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

fn display_units(ch: char) -> usize {
    if ch.is_ascii() {
        1
    } else {
        2
    }
}

fn search_result_card(
    user: &SearchUserVm,
    selected: bool,
    feedback: Option<&str>,
) -> Element<'static, AppMessage> {
    let title = truncate_single_line(
        if user.nickname.trim().is_empty() {
            &user.username
        } else {
            &user.nickname
        },
        28,
    );
    let subtitle = format!("Weixin ID: {}", user.username);
    let tip = feedback
        .unwrap_or("点击 Add to Contacts 发送好友申请")
        .to_string();

    let action: Element<'static, AppMessage> = if user.is_friend {
        container(
            text("Added")
                .size(15)
                .color(Color::from_rgb8(0xA8, 0xAF, 0xB8)),
        )
        .padding([8, 12])
        .into()
    } else {
        button(
            text("Add to Contacts")
                .size(16)
                .color(Color::from_rgb8(0xE9, 0xEE, 0xF5)),
        )
        .padding([10, 16])
        .style(add_to_contacts_button_style)
        .on_press(AppMessage::AddFriendRequestPressed)
        .into()
    };

    container(
        column![
            row![
                avatar_square(&title),
                column![
                    text(title).size(22).color(C_TEXT_PRIMARY),
                    text(subtitle).size(13).color(C_TEXT_SECONDARY),
                    text(format!("Search Session: {}", user.search_session_id))
                        .size(13)
                        .color(C_TEXT_SECONDARY)
                ]
                .spacing(5)
                .width(Length::Fill),
            ]
            .spacing(14)
            .align_y(alignment::Vertical::Center),
            section_divider(),
            text(tip).size(14).color(Color::from_rgb8(0xC8, 0xCF, 0xD8)),
            container(action)
                .width(Length::Fill)
                .align_x(alignment::Horizontal::Center),
        ]
        .spacing(16),
    )
    .padding([18, 16])
    .width(Length::Fill)
    .style(move |_| container::Style {
        background: Some(Background::Color(C_POPUP_CARD_BG)),
        border: border::rounded(14.0).width(1.0).color(if selected {
            Color::from_rgb8(0x4B, 0x89, 0xD0)
        } else {
            Color::from_rgb8(0x3C, 0x42, 0x4A)
        }),
        ..container::Style::default()
    })
    .into()
}

fn search_result_tile(user: &SearchUserVm, selected: bool) -> Element<'static, AppMessage> {
    let title = truncate_single_line(
        if user.nickname.trim().is_empty() {
            &user.username
        } else {
            &user.nickname
        },
        24,
    );

    button(
        row![
            avatar_square(&title),
            text(title).size(15).color(C_TEXT_PRIMARY),
            container(
                text(if user.is_friend { "Added" } else { "查看" })
                    .size(13)
                    .color(if user.is_friend {
                        Color::from_rgb8(0x9E, 0xA5, 0xAE)
                    } else {
                        Color::from_rgb8(0xDF, 0xE5, 0xEC)
                    })
            )
            .width(Length::Fill)
            .align_x(alignment::Horizontal::Right),
        ]
        .spacing(10)
        .align_y(alignment::Vertical::Center),
    )
    .width(Length::Fill)
    .padding([8, 10])
    .style(move |_theme: &Theme, status| {
        let bg = match status {
            button::Status::Hovered => Some(Background::Color(Color::from_rgb8(0x33, 0x37, 0x3E))),
            button::Status::Pressed => Some(Background::Color(Color::from_rgb8(0x30, 0x34, 0x3B))),
            _ => {
                if selected {
                    Some(Background::Color(Color::from_rgb8(0x31, 0x35, 0x3D)))
                } else {
                    None
                }
            }
        };
        button::Style {
            background: bg,
            text_color: Color::TRANSPARENT,
            border: border::rounded(8.0),
            shadow: Default::default(),
            snap: true,
        }
    })
    .on_press(AppMessage::AddFriendResultSelected {
        user_id: user.user_id,
    })
    .into()
}

fn add_to_contacts_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb8(0x4A, 0x4E, 0x56),
        button::Status::Pressed => Color::from_rgb8(0x40, 0x44, 0x4B),
        _ => Color::from_rgb8(0x3D, 0x41, 0x48),
    };

    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::from_rgb8(0xE8, 0xED, 0xF3),
        border: border::rounded(9.0),
        shadow: Default::default(),
        snap: true,
    }
}
