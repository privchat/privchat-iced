use iced::widget::{button, column, container, row, scrollable, text};
use iced::{alignment, border, Background, Color, Element, Length, Theme};

use crate::app::message::AppMessage;
use crate::app::state::SwitchAccountState;

const PAGE_BG: Color = Color::from_rgb8(0x1B, 0x1F, 0x26);
const PANEL_BG: Color = Color::from_rgb8(0x24, 0x28, 0x31);
const PANEL_BORDER: Color = Color::from_rgb8(0x39, 0x3F, 0x4A);
const DIVIDER: Color = Color::from_rgb8(0x34, 0x39, 0x43);

pub fn view(state: &SwitchAccountState) -> Element<'_, AppMessage> {
    let mut list = column![].spacing(8);

    if state.loading && state.accounts.is_empty() {
        list = list.push(
            text("加载本地账号中...")
                .size(14)
                .color(Color::from_rgb8(0x9D, 0xA5, 0xB1)),
        );
    } else if state.accounts.is_empty() {
        list = list.push(
            text("暂无本地账号")
                .size(14)
                .color(Color::from_rgb8(0x9D, 0xA5, 0xB1)),
        );
    } else {
        for account in &state.accounts {
            let is_switching = state
                .switching_uid
                .as_ref()
                .map(|uid| uid == &account.uid)
                .unwrap_or(false);
            let status_text = if is_switching {
                "切换中..."
            } else if account.is_active {
                "当前"
            } else {
                "切换"
            };

            let label = row![
                column![
                    text(format!("账号 {}", account.uid))
                        .size(16)
                        .color(Color::from_rgb8(0xE7, 0xEC, 0xF5)),
                    text(format!("UID {}", account.uid))
                        .size(13)
                        .color(Color::from_rgb8(0x93, 0x9A, 0xA4)),
                ]
                .spacing(3),
                container(text(status_text).size(13).color(if account.is_active {
                    Color::from_rgb8(0xD9, 0x98, 0x35)
                } else {
                    Color::from_rgb8(0x78, 0xAE, 0xF8)
                }))
                .width(Length::Fill)
                .align_x(alignment::Horizontal::Right)
            ]
            .align_y(alignment::Vertical::Center);

            let on_press = if account.is_active || state.loading {
                AppMessage::Noop
            } else {
                AppMessage::SwitchAccountPressed {
                    uid: account.uid.clone(),
                }
            };

            list = list.push(
                button(container(label).width(Length::Fill).padding([10, 12]))
                    .width(Length::Fill)
                    .style(account_item_style)
                    .on_press(on_press),
            );
        }
    }

    let mut root = column![
        row![
            text("切换账号")
                .size(30)
                .color(Color::from_rgb8(0xEB, 0xF0, 0xF7)),
            container(text("")).width(Length::Fill),
            button("关闭")
                .padding([8, 14])
                .style(secondary_button_style)
                .on_press(AppMessage::CloseSwitchAccountPanel),
        ]
        .align_y(alignment::Vertical::Center),
        container(scrollable(list).height(Length::Fill))
            .padding([8, 0])
            .height(Length::Fill),
        container(text(""))
            .height(Length::Fixed(1.0))
            .style(|_| container::Style {
                background: Some(Background::Color(DIVIDER)),
                ..container::Style::default()
            }),
        row![
            container(text("")).width(Length::Fill),
            button("添加账号")
                .padding([10, 22])
                .style(primary_button_style)
                .on_press(AppMessage::SwitchAccountAddPressed)
        ],
    ]
    .spacing(12);

    if let Some(error) = &state.error {
        root = root.push(
            text(error)
                .size(13)
                .color(Color::from_rgb8(0xE4, 0x8C, 0x8C)),
        );
    }

    container(
        container(root.padding([20, 20]))
            .style(|_| container::Style {
                background: Some(Background::Color(PANEL_BG)),
                border: border::rounded(12.0).width(1.0).color(PANEL_BORDER),
                ..container::Style::default()
            })
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .padding([28, 28])
    .width(Length::Fill)
    .height(Length::Fill)
    .style(|_| container::Style {
        background: Some(Background::Color(PAGE_BG)),
        ..container::Style::default()
    })
    .into()
}

fn account_item_style(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered => Some(Background::Color(Color::from_rgb8(0x36, 0x3B, 0x45))),
        button::Status::Pressed => Some(Background::Color(Color::from_rgb8(0x40, 0x46, 0x52))),
        _ => Some(Background::Color(Color::from_rgb8(0x2D, 0x32, 0x3B))),
    };
    button::Style {
        background,
        text_color: Color::TRANSPARENT,
        border: border::rounded(10.0),
        shadow: Default::default(),
        snap: true,
    }
}

fn primary_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered => Color::from_rgb8(0xD0, 0x84, 0x22),
        button::Status::Pressed => Color::from_rgb8(0xB2, 0x6C, 0x15),
        _ => Color::from_rgb8(0xC2, 0x76, 0x19),
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: Color::from_rgb8(0xF7, 0xFA, 0xFF),
        border: border::rounded(9.0),
        shadow: Default::default(),
        snap: true,
    }
}

fn secondary_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered => Some(Background::Color(Color::from_rgb8(0x3A, 0x40, 0x4A))),
        button::Status::Pressed => Some(Background::Color(Color::from_rgb8(0x45, 0x4C, 0x57))),
        _ => Some(Background::Color(Color::from_rgb8(0x2F, 0x34, 0x3D))),
    };
    button::Style {
        background,
        text_color: Color::from_rgb8(0xDA, 0xE0, 0xE9),
        border: border::rounded(8.0),
        shadow: Default::default(),
        snap: true,
    }
}
