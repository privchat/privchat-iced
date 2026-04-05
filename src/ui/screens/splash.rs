use iced::widget::{column, container, text};
use iced::{alignment, border, Background, Color, Element, Length};

use crate::app::message::AppMessage;
use crate::ui::icons::{self, Icon};

const C_BG: Color = Color::from_rgb8(0x16, 0x19, 0x1F);
const C_ICON_BG: Color = Color::from_rgb8(0x45, 0xC1, 0x73);
const C_TITLE: Color = Color::from_rgb8(0xEC, 0xF0, 0xF4);
const C_SUBTITLE: Color = Color::from_rgb8(0x8C, 0x95, 0xA0);

pub fn view() -> Element<'static, AppMessage> {
    let logo = container(icons::render(
        Icon::Message,
        72.0,
        Color::from_rgb8(0xF7, 0xFF, 0xFA),
    ))
    .width(Length::Fixed(104.0))
    .height(Length::Fixed(104.0))
    .align_x(alignment::Horizontal::Center)
    .align_y(alignment::Vertical::Center)
    .style(|_| container::Style {
        background: Some(Background::Color(C_ICON_BG)),
        border: border::rounded(24.0),
        ..container::Style::default()
    });

    container(
        column![
            logo,
            text("PrivChat").size(34).color(C_TITLE),
            text("正在启动...").size(14).color(C_SUBTITLE)
        ]
        .spacing(18)
        .align_x(alignment::Horizontal::Center),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .style(|_| container::Style {
        background: Some(Background::Color(C_BG)),
        ..container::Style::default()
    })
    .into()
}
