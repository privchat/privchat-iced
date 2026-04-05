use iced::widget::{button, column, container, row, text, text_input};
use iced::{Element, Length};

use crate::app::message::AppMessage;
use crate::app::state::AuthState;

pub fn view(auth: &AuthState) -> Element<'_, AppMessage> {
    let mut content = column![
        text("PrivChat Login").size(28),
        text_input("Username", &auth.username)
            .on_input(|text| AppMessage::LoginUsernameChanged { text }),
        text_input("Password", &auth.password)
            .secure(true)
            .on_submit(AppMessage::LoginPressed)
            .on_input(|text| AppMessage::LoginPasswordChanged { text }),
        text_input("Device ID", &auth.device_id)
            .on_submit(AppMessage::LoginPressed)
            .on_input(|text| AppMessage::LoginDeviceIdChanged { text }),
    ]
    .spacing(10);

    let login_button = if auth.is_submitting {
        button("Logging in...")
    } else {
        button("Login").on_press(AppMessage::LoginPressed)
    };
    let register_button = if auth.is_submitting {
        button("Registering...")
    } else {
        button("Register").on_press(AppMessage::RegisterPressed)
    };

    content = content.push(row![login_button, register_button].spacing(8));

    if let Some(error) = &auth.error {
        content = content.push(text(error));
    }
    if let Some(user_id) = auth.user_id {
        content = content.push(text(format!("Logged in as user_id={user_id}")).size(12));
    }

    container(content.padding(16).max_width(520))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
}
