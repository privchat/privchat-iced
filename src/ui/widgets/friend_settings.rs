use iced::widget::{button, column, container, row, text, text_input};
use iced::{alignment, border, Background, Color, Element, Length};

use crate::app::message::AppMessage;
use crate::app::state::FriendSettingsState;

const C_CARD_BG: Color = Color::from_rgb8(0x25, 0x2A, 0x31);
const C_CARD_BORDER: Color = Color::from_rgb8(0x3A, 0x41, 0x4B);
const C_SECTION_BG: Color = Color::from_rgb8(0x2A, 0x2F, 0x37);
const C_TEXT_PRIMARY: Color = Color::from_rgb8(0xE0, 0xE4, 0xE9);
const C_TEXT_SECONDARY: Color = Color::from_rgb8(0x8B, 0x93, 0x9E);
const C_ACCENT: Color = Color::from_rgb8(0x2F, 0x7F, 0xD6);
const C_DANGER: Color = Color::from_rgb8(0xEA, 0x4B, 0x52);
const C_SWITCH_ON: Color = Color::from_rgb8(0x2F, 0x7F, 0xD6);
const C_SWITCH_OFF: Color = Color::from_rgb8(0x4A, 0x52, 0x5E);
const C_ROW_HOVER: Color = Color::from_rgb8(0x2F, 0x35, 0x3E);
const C_DIVIDER: Color = Color::from_rgb8(0x35, 0x39, 0x40);

pub fn view(state: &FriendSettingsState) -> Element<'_, AppMessage> {
    let header = column![
        text("好友设置").size(16).color(C_TEXT_PRIMARY),
        text(state.title.clone()).size(13).color(C_TEXT_SECONDARY),
    ]
    .spacing(6);

    let mut body = column![header].spacing(14);

    if state.loading {
        body = body.push(
            container(text("加载中…").size(12).color(C_TEXT_SECONDARY))
                .padding([16, 12])
                .width(Length::Fill)
                .center_x(Length::Fill),
        );
    }

    body = body.push(remark_section(state));
    body = body.push(switches_section(state));
    body = body.push(delete_section(state));

    if let Some(err) = &state.error {
        body = body.push(text(err.clone()).size(12).color(C_DANGER));
    }

    body = body.push(footer(state));

    let card: Element<'_, AppMessage> = container(body.padding(20))
        .max_width(460.0)
        .width(Length::Fixed(460.0))
        .style(|_| container::Style {
            background: Some(Background::Color(C_CARD_BG)),
            border: border::width(1.0).color(C_CARD_BORDER).rounded(12.0),
            ..container::Style::default()
        })
        .into();

    if state.delete_confirm_open {
        iced::widget::stack![card, confirm_dialog(state)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        card
    }
}

fn remark_section(state: &FriendSettingsState) -> Element<'_, AppMessage> {
    let content: Element<'_, AppMessage> = if state.editing_remark {
        let input = text_input("设置备注", &state.remark_input)
            .on_input(AppMessage::FriendSettingsRemarkInputChanged)
            .on_submit(AppMessage::FriendSettingsRemarkSubmitPressed)
            .padding([8, 10])
            .size(13)
            .width(Length::Fill);
        let submit_label = if state.submitting_remark {
            "保存中…"
        } else {
            "保存"
        };
        let mut submit = button(text(submit_label).size(13).color(Color::WHITE))
            .padding([6, 14])
            .style(accent_button_style);
        if !state.submitting_remark {
            submit = submit.on_press(AppMessage::FriendSettingsRemarkSubmitPressed);
        }
        let cancel = button(text("取消").size(13).color(C_TEXT_PRIMARY))
            .padding([6, 14])
            .on_press(AppMessage::FriendSettingsRemarkEditCancelled)
            .style(ghost_button_style);
        column![
            text("备注").size(12).color(C_TEXT_SECONDARY),
            input,
            row![container(text("")).width(Length::Fill), cancel, submit].spacing(8),
        ]
        .spacing(8)
        .into()
    } else {
        let display = if state.remark.is_empty() {
            "未设置".to_string()
        } else {
            state.remark.clone()
        };
        let edit_btn = button(
            row![
                column![
                    text("备注").size(12).color(C_TEXT_SECONDARY),
                    text(display).size(13).color(C_TEXT_PRIMARY),
                ]
                .spacing(4)
                .width(Length::Fill),
                text(">").size(13).color(C_TEXT_SECONDARY),
            ]
            .align_y(alignment::Vertical::Center)
            .padding([10, 12]),
        )
        .width(Length::Fill)
        .padding(0)
        .on_press(AppMessage::FriendSettingsRemarkEditPressed)
        .style(cell_button_style);
        edit_btn.into()
    };

    section_wrap(content)
}

fn switches_section(state: &FriendSettingsState) -> Element<'_, AppMessage> {
    let mute_row = switch_row(
        "免打扰",
        state.is_muted,
        state.submitting_mute || state.direct_channel_id.is_none(),
        AppMessage::FriendSettingsMuteToggled(!state.is_muted),
    );
    let divider = container(text(""))
        .width(Length::Fill)
        .height(Length::Fixed(1.0))
        .style(|_| container::Style {
            background: Some(Background::Color(C_DIVIDER)),
            ..container::Style::default()
        });
    let block_row = switch_row(
        "加入黑名单",
        state.is_blacklisted,
        state.submitting_block || state.loading,
        AppMessage::FriendSettingsBlockToggled(!state.is_blacklisted),
    );

    section_wrap(column![mute_row, divider, block_row].into())
}

fn switch_row<'a>(
    label: &str,
    checked: bool,
    disabled: bool,
    on_toggle: AppMessage,
) -> Element<'a, AppMessage> {
    let switch = container(
        container(text(""))
            .width(Length::Fixed(16.0))
            .height(Length::Fixed(16.0))
            .style(|_| container::Style {
                background: Some(Background::Color(Color::WHITE)),
                border: border::rounded(8.0),
                ..container::Style::default()
            }),
    )
    .width(Length::Fixed(40.0))
    .height(Length::Fixed(20.0))
    .padding([2, 2])
    .align_x(if checked {
        alignment::Horizontal::Right
    } else {
        alignment::Horizontal::Left
    })
    .align_y(alignment::Vertical::Center)
    .style(move |_| container::Style {
        background: Some(Background::Color(if checked {
            C_SWITCH_ON
        } else {
            C_SWITCH_OFF
        })),
        border: border::rounded(10.0),
        ..container::Style::default()
    });

    let content = row![
        text(label.to_string()).size(13).color(C_TEXT_PRIMARY).width(Length::Fill),
        switch,
    ]
    .align_y(alignment::Vertical::Center)
    .padding([12, 12]);

    let mut btn = button(content)
        .width(Length::Fill)
        .padding(0)
        .style(cell_button_style);
    if !disabled {
        btn = btn.on_press(on_toggle);
    }
    btn.into()
}

fn delete_section(state: &FriendSettingsState) -> Element<'_, AppMessage> {
    let label = if state.submitting_delete {
        "删除中…"
    } else {
        "删除联系人"
    };
    let mut btn = button(
        container(text(label.to_string()).size(13).color(C_DANGER))
            .width(Length::Fill)
            .center_x(Length::Fill)
            .padding([12, 12]),
    )
    .width(Length::Fill)
    .padding(0)
    .style(cell_button_style);
    if !state.submitting_delete {
        btn = btn.on_press(AppMessage::FriendSettingsDeletePressed);
    }
    section_wrap(btn.into())
}

fn footer(_state: &FriendSettingsState) -> Element<'_, AppMessage> {
    let close = button(text("关闭").size(13).color(C_TEXT_PRIMARY))
        .padding([6, 18])
        .on_press(AppMessage::DismissFriendSettings)
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

fn confirm_dialog(state: &FriendSettingsState) -> Element<'_, AppMessage> {
    let backdrop = iced::widget::mouse_area(
        container(text(""))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(Background::Color(Color::from_rgba8(0, 0, 0, 0.4))),
                ..container::Style::default()
            }),
    )
    .on_press(AppMessage::FriendSettingsDeleteCancelled);

    let cancel = button(text("取消").size(13).color(C_TEXT_PRIMARY))
        .padding([6, 18])
        .on_press(AppMessage::FriendSettingsDeleteCancelled)
        .style(ghost_button_style);
    let confirm_label = if state.submitting_delete {
        "删除中…"
    } else {
        "确定"
    };
    let mut confirm = button(text(confirm_label.to_string()).size(13).color(Color::WHITE))
        .padding([6, 18])
        .style(danger_button_style);
    if !state.submitting_delete {
        confirm = confirm.on_press(AppMessage::FriendSettingsDeleteConfirmed);
    }

    let dialog = container(
        column![
            text("确认删除").size(15).color(C_TEXT_PRIMARY),
            text("删除该联系人后，你们之间的聊天将被清空。")
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
        button::Status::Hovered | button::Status::Pressed => Some(Background::Color(C_ROW_HOVER)),
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

