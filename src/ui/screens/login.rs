use iced::widget::{button, column, container, pick_list, row, text, text_input};
use iced::{Element, Length};

use crate::app::message::AppMessage;
use crate::app::state::AuthState;
use crate::config::AccountMode;

pub fn view<'a>(
    auth: &'a AuthState,
    add_account_mode: bool,
    account_mode: AccountMode,
) -> Element<'a, AppMessage> {
    match account_mode {
        AccountMode::Builtin => view_builtin(auth, add_account_mode),
        AccountMode::Platform => view_platform(auth, add_account_mode),
    }
}

fn view_builtin(auth: &AuthState, add_account_mode: bool) -> Element<'_, AppMessage> {
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

/// 国家拨号区号选项（与 privchat-app `LoginPagePlatform.kt::countryOptions` 同步）。
/// 长期方向：从 privchat-application 拉取动态列表。
#[derive(Debug, Clone, PartialEq, Eq)]
struct CountryOption {
    label: &'static str,
    dial_code: &'static str,
}

impl std::fmt::Display for CountryOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.label, self.dial_code)
    }
}

const COUNTRY_OPTIONS: &[CountryOption] = &[
    CountryOption {
        label: "🇨🇳 中国大陆",
        dial_code: "+86",
    },
    CountryOption {
        label: "🇭🇰 中国香港",
        dial_code: "+852",
    },
    CountryOption {
        label: "🇹🇼 中国台湾",
        dial_code: "+886",
    },
    CountryOption {
        label: "🇺🇸 United States",
        dial_code: "+1",
    },
    CountryOption {
        label: "🇸🇬 Singapore",
        dial_code: "+65",
    },
    CountryOption {
        label: "🇲🇾 Malaysia",
        dial_code: "+60",
    },
    CountryOption {
        label: "🇯🇵 日本",
        dial_code: "+81",
    },
    CountryOption {
        label: "🇰🇷 한국",
        dial_code: "+82",
    },
    CountryOption {
        label: "🇬🇧 United Kingdom",
        dial_code: "+44",
    },
    CountryOption {
        label: "🇦🇺 Australia",
        dial_code: "+61",
    },
];

fn view_platform(auth: &AuthState, add_account_mode: bool) -> Element<'_, AppMessage> {
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
        column![
            text("PrivChat").size(28),
            text("手机号一键登录 / 注册").size(13),
        ]
        .spacing(4)
        .into()
    };

    let selected_country = COUNTRY_OPTIONS
        .iter()
        .find(|c| c.dial_code == auth.country_dial_code)
        .cloned()
        .unwrap_or_else(|| COUNTRY_OPTIONS[0].clone());

    let country_picker = pick_list(
        COUNTRY_OPTIONS,
        Some(selected_country),
        |selected: CountryOption| AppMessage::LoginCountryDialCodeSelected {
            dial_code: selected.dial_code.to_string(),
        },
    )
    .placeholder("选择国家/地区")
    .width(Length::FillPortion(1));

    let phone_input = text_input("手机号", &auth.mobile)
        .on_input(|text| AppMessage::LoginMobileChanged { text })
        .width(Length::FillPortion(2));

    let mobile_row = row![country_picker, phone_input]
        .spacing(8)
        .align_y(iced::alignment::Vertical::Center);

    let sms_input = text_input("短信验证码", &auth.sms_code)
        .on_input(|text| AppMessage::LoginSmsCodeChanged { text })
        .on_submit(AppMessage::LoginSmsLoginPressed)
        .width(Length::FillPortion(2));

    let sms_button_label: String = if auth.is_sending_sms {
        "发送中...".to_string()
    } else if auth.sms_cooldown_secs > 0 {
        format!("{}s", auth.sms_cooldown_secs)
    } else {
        "获取验证码".to_string()
    };
    let mut sms_button = button(text(sms_button_label).size(13)).padding([8, 12]);
    if !auth.is_sending_sms && auth.sms_cooldown_secs == 0 {
        sms_button = sms_button.on_press(AppMessage::LoginSendSmsPressed);
    }

    let sms_row = row![sms_input, sms_button]
        .spacing(8)
        .align_y(iced::alignment::Vertical::Center);

    let device_input = text_input("设备 ID", &auth.device_id)
        .on_input(|text| AppMessage::LoginDeviceIdChanged { text });

    let mut content = column![title, mobile_row, sms_row, device_input].spacing(10);

    let primary_button = if auth.is_submitting {
        button("登录中...").padding([8, 16])
    } else {
        button(text("继续").size(14))
            .padding([8, 16])
            .on_press(AppMessage::LoginSmsLoginPressed)
    };
    content = content.push(primary_button);

    if let Some(error) = &auth.error {
        content = content.push(text(error));
    }
    if auth.user_id.is_some() && !auth.mobile.trim().is_empty() {
        content = content.push(
            text(format!(
                "当前登录手机号：{}{}",
                auth.country_dial_code, auth.mobile
            ))
            .size(12),
        );
    }

    container(content.padding(16).max_width(520))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
}
