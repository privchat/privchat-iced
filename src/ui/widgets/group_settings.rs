use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{alignment, border, Background, Color, Element, Length};

use crate::app::message::AppMessage;
use crate::app::state::GroupSettingsState;
use crate::presentation::vm::GroupMemberDetailVm;

const C_CARD_BG: Color = Color::from_rgb8(0x25, 0x2A, 0x31);
const C_CARD_BORDER: Color = Color::from_rgb8(0x3A, 0x41, 0x4B);
const C_SECTION_BG: Color = Color::from_rgb8(0x2A, 0x2F, 0x37);
const C_TEXT_PRIMARY: Color = Color::from_rgb8(0xE0, 0xE4, 0xE9);
const C_TEXT_SECONDARY: Color = Color::from_rgb8(0x8B, 0x93, 0x9E);
const C_ACCENT: Color = Color::from_rgb8(0x2F, 0x7F, 0xD6);
const C_DANGER: Color = Color::from_rgb8(0xEA, 0x4B, 0x52);
const C_ROLE_OWNER: Color = Color::from_rgb8(0xE8, 0xA0, 0x3C);
const C_ROLE_ADMIN: Color = Color::from_rgb8(0x64, 0xB5, 0xF6);
const C_DIVIDER: Color = Color::from_rgb8(0x35, 0x39, 0x40);

pub fn view(state: &GroupSettingsState) -> Element<'_, AppMessage> {
    let header = column![
        text("群管理").size(16).color(C_TEXT_PRIMARY),
        text(state.title.clone()).size(13).color(C_TEXT_SECONDARY),
        text(format!("共 {} 位成员", state.members.len()))
            .size(12)
            .color(C_TEXT_SECONDARY),
    ]
    .spacing(4);

    let mut body = column![header].spacing(14);

    if state.loading {
        body = body.push(
            container(text("加载中…").size(12).color(C_TEXT_SECONDARY))
                .padding([16, 12])
                .width(Length::Fill)
                .center_x(Length::Fill),
        );
    }

    if state.is_admin() {
        body = body.push(invite_section(state));
    }

    body = body.push(members_section(state));
    body = body.push(leave_section(state));

    if let Some(err) = &state.error {
        body = body.push(text(err.clone()).size(12).color(C_DANGER));
    }

    body = body.push(footer());

    let card: Element<'_, AppMessage> = container(body.padding(20))
        .max_width(520.0)
        .width(Length::Fixed(520.0))
        .style(|_| container::Style {
            background: Some(Background::Color(C_CARD_BG)),
            border: border::width(1.0).color(C_CARD_BORDER).rounded(12.0),
            ..container::Style::default()
        })
        .into();

    if state.leave_confirm_open {
        iced::widget::stack![card, confirm_dialog(state)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        card
    }
}

fn invite_section(state: &GroupSettingsState) -> Element<'_, AppMessage> {
    let input = text_input("输入用户 ID 邀请入群", &state.invite_input)
        .on_input(AppMessage::GroupSettingsInviteInputChanged)
        .on_submit(AppMessage::GroupSettingsInviteSubmitPressed)
        .padding([8, 10])
        .size(13)
        .width(Length::Fill);

    let label = if state.submitting_invite {
        "邀请中…"
    } else {
        "邀请"
    };
    let mut submit = button(text(label).size(13).color(Color::WHITE))
        .padding([8, 16])
        .style(accent_button_style);
    if !state.submitting_invite {
        submit = submit.on_press(AppMessage::GroupSettingsInviteSubmitPressed);
    }

    section_wrap(
        column![
            text("邀请新成员").size(12).color(C_TEXT_SECONDARY),
            row![input, submit].spacing(8),
        ]
        .spacing(8)
        .padding([12, 12])
        .into(),
    )
}

fn members_section(state: &GroupSettingsState) -> Element<'_, AppMessage> {
    if state.members.is_empty() {
        return section_wrap(
            container(text("暂无成员").size(12).color(C_TEXT_SECONDARY))
                .padding([16, 12])
                .width(Length::Fill)
                .center_x(Length::Fill)
                .into(),
        );
    }

    let mut list = column![].spacing(0);
    let total = state.members.len();
    for (idx, member) in state.members.iter().enumerate() {
        list = list.push(member_row(state, member));
        if idx + 1 < total {
            list = list.push(divider());
        }
    }

    let scroll = scrollable(list)
        .height(Length::Fixed(320.0))
        .width(Length::Fill);
    section_wrap(scroll.into())
}

fn member_row<'a>(
    state: &'a GroupSettingsState,
    member: &'a GroupMemberDetailVm,
) -> Element<'a, AppMessage> {
    let role_chip: Element<'_, AppMessage> = match member.role.as_str() {
        "owner" => role_badge("群主", C_ROLE_OWNER).into(),
        "admin" => role_badge("管理员", C_ROLE_ADMIN).into(),
        _ => container(text("")).into(),
    };

    let name_line = row![
        text(member.display_name.clone())
            .size(13)
            .color(C_TEXT_PRIMARY),
        role_chip,
    ]
    .spacing(8)
    .align_y(alignment::Vertical::Center);

    let subtitle = text(format!("UID: {}", member.user_id))
        .size(11)
        .color(C_TEXT_SECONDARY);

    let info = column![name_line, subtitle]
        .spacing(2)
        .width(Length::Fill);

    let show_remove = state.is_admin()
        && member.user_id != state.my_user_id
        && member.role != "owner";

    let action: Element<'_, AppMessage> = if show_remove {
        let is_submitting = state.submitting_remove == Some(member.user_id);
        let label = if is_submitting { "移除中…" } else { "移除" };
        let mut btn = button(text(label).size(12).color(Color::WHITE))
            .padding([4, 12])
            .style(danger_button_style);
        if !is_submitting {
            btn = btn.on_press(AppMessage::GroupSettingsRemoveMemberPressed(member.user_id));
        }
        btn.into()
    } else {
        container(text("")).into()
    };

    row![info, action]
        .align_y(alignment::Vertical::Center)
        .padding([10, 12])
        .spacing(8)
        .into()
}

fn role_badge<'a>(label: &'a str, color: Color) -> Element<'a, AppMessage> {
    container(text(label).size(10).color(Color::WHITE))
        .padding([2, 6])
        .style(move |_| container::Style {
            background: Some(Background::Color(color)),
            border: border::rounded(4.0),
            ..container::Style::default()
        })
        .into()
}

fn divider<'a>() -> Element<'a, AppMessage> {
    container(text(""))
        .width(Length::Fill)
        .height(Length::Fixed(1.0))
        .style(|_| container::Style {
            background: Some(Background::Color(C_DIVIDER)),
            ..container::Style::default()
        })
        .into()
}

fn leave_section(state: &GroupSettingsState) -> Element<'_, AppMessage> {
    let label = if state.submitting_leave {
        "退出中…"
    } else {
        "退出群组"
    };
    let mut btn = button(
        container(text(label).size(13).color(C_DANGER))
            .width(Length::Fill)
            .center_x(Length::Fill)
            .padding([12, 12]),
    )
    .width(Length::Fill)
    .padding(0)
    .style(cell_button_style);
    if !state.submitting_leave {
        btn = btn.on_press(AppMessage::GroupSettingsLeavePressed);
    }
    section_wrap(btn.into())
}

fn footer<'a>() -> Element<'a, AppMessage> {
    let close = button(text("关闭").size(13).color(C_TEXT_PRIMARY))
        .padding([6, 18])
        .on_press(AppMessage::DismissGroupSettings)
        .style(ghost_button_style);
    row![container(text("")).width(Length::Fill), close]
        .align_y(alignment::Vertical::Center)
        .into()
}

fn section_wrap<'a>(inner: Element<'a, AppMessage>) -> Element<'a, AppMessage> {
    container(inner)
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(C_SECTION_BG)),
            border: border::rounded(8.0),
            ..container::Style::default()
        })
        .into()
}

fn confirm_dialog(state: &GroupSettingsState) -> Element<'_, AppMessage> {
    let backdrop = iced::widget::mouse_area(
        container(text(""))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(Background::Color(Color::from_rgba8(0, 0, 0, 0.4))),
                ..container::Style::default()
            }),
    )
    .on_press(AppMessage::GroupSettingsLeaveCancelled);

    let cancel = button(text("取消").size(13).color(C_TEXT_PRIMARY))
        .padding([6, 18])
        .on_press(AppMessage::GroupSettingsLeaveCancelled)
        .style(ghost_button_style);
    let confirm_label = if state.submitting_leave {
        "退出中…"
    } else {
        "确定退出"
    };
    let mut confirm = button(text(confirm_label).size(13).color(Color::WHITE))
        .padding([6, 18])
        .style(danger_button_style);
    if !state.submitting_leave {
        confirm = confirm.on_press(AppMessage::GroupSettingsLeaveConfirmed);
    }

    let dialog = container(
        column![
            text("确认退出").size(15).color(C_TEXT_PRIMARY),
            text("退出后将不再接收本群消息，群内聊天记录会被清空。")
                .size(12)
                .color(C_TEXT_SECONDARY),
            row![container(text("")).width(Length::Fill), cancel, confirm].spacing(8),
        ]
        .spacing(12)
        .padding(20),
    )
    .max_width(360.0)
    .style(|_| container::Style {
        background: Some(Background::Color(C_CARD_BG)),
        border: border::width(1.0).color(C_CARD_BORDER).rounded(10.0),
        ..container::Style::default()
    });

    iced::widget::stack![
        backdrop,
        container(dialog)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(alignment::Horizontal::Center)
            .align_y(alignment::Vertical::Center),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn accent_button_style(_theme: &iced::Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => Color::from_rgb8(0x2A, 0x6C, 0xB2),
        _ => C_ACCENT,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        border: border::rounded(6.0),
        ..button::Style::default()
    }
}

fn ghost_button_style(_theme: &iced::Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => Color::from_rgb8(0x3A, 0x41, 0x4B),
        _ => Color::from_rgb8(0x2F, 0x35, 0x3E),
    };
    button::Style {
        background: Some(Background::Color(bg)),
        border: border::width(1.0)
            .color(Color::from_rgb8(0x4A, 0x52, 0x5E))
            .rounded(6.0),
        ..button::Style::default()
    }
}

fn danger_button_style(_theme: &iced::Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => Color::from_rgb8(0xB8, 0x3A, 0x41),
        _ => C_DANGER,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        border: border::rounded(6.0),
        ..button::Style::default()
    }
}

fn cell_button_style(_theme: &iced::Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => {
            Some(Background::Color(Color::from_rgb8(0x2F, 0x35, 0x3E)))
        }
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
