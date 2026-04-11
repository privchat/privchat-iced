use iced::widget::{button, column, container, row, text, text_input};
use iced::{Element, Length};

use crate::app::message::AppMessage;
use crate::app::state::AuthState;

pub fn view(auth: &AuthState, add_account_mode: bool) -> Element<'_, AppMessage> {
    let title: Element<'_, AppMessage> = if add_account_mode {
        row![
            button("←")
                .padding([6, 10])
                .on_press(AppMessage::LoginBackPressed),
            text("添加账号").size(28),
        ]
        .spacing(10)
        .align_y(iced::alignment::Vertical::Center)
        .into()
    } else {
        text("PrivChat 登录").size(28).into()
    };

    let mut content = column![
        title,
        text_input(
            if add_account_mode {
                "PrivChat ID / 用户名"
            } else {
                "用户名"
            },
            &auth.username
        )
        .on_input(|text| AppMessage::LoginUsernameChanged { text }),
        text_input("密码", &auth.password)
            .secure(true)
            .on_submit(AppMessage::LoginPressed)
            .on_input(|text| AppMessage::LoginPasswordChanged { text }),
        text_input("设备 ID", &auth.device_id)
            .on_submit(AppMessage::LoginPressed)
            .on_input(|text| AppMessage::LoginDeviceIdChanged { text }),
    ]
    .spacing(10);

    let login_button = if auth.is_submitting {
        button("登录中...")
    } else {
        button("登录").on_press(AppMessage::LoginPressed)
    };
    if add_account_mode {
        content = content.push(row![login_button].spacing(8));
    } else {
        let register_button = if auth.is_submitting {
            button("注册中...")
        } else {
            button("注册").on_press(AppMessage::RegisterPressed)
        };
        content = content.push(row![login_button, register_button].spacing(8));
    }

    if let Some(error) = &auth.error {
        content = content.push(text(error));
    }
    if auth.user_id.is_some() && !auth.username.trim().is_empty() {
        content = content.push(text(format!("当前登录账号：{}", auth.username.trim())).size(12));
    }

    container(content.padding(16).max_width(520))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
}
