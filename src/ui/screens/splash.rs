use std::path::Path;

use iced::widget::{column, container, image, text};
use iced::{alignment, Background, Color, ContentFit, Element, Length};

use crate::app::message::AppMessage;

const C_BG: Color = Color::from_rgb8(0x16, 0x19, 0x1F);
const C_TITLE: Color = Color::from_rgb8(0xEC, 0xF0, 0xF4);
const C_SUBTITLE: Color = Color::from_rgb8(0x8C, 0x95, 0xA0);

pub fn view() -> Element<'static, AppMessage> {
    let logo_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/privchat-gold.png");

    let logo = container(
        image(image::Handle::from_path(logo_path))
            .width(Length::Fixed(120.0))
            .height(Length::Fixed(120.0))
            .content_fit(ContentFit::Contain),
    )
    .width(Length::Fixed(128.0))
    .height(Length::Fixed(128.0))
    .align_x(alignment::Horizontal::Center)
    .align_y(alignment::Vertical::Center);

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
