use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Background, Color, Element, Length, border};

use crate::app::message::AppMessage;

pub fn view(logs: Vec<String>, feedback: Option<String>) -> Element<'static, AppMessage> {
    let toolbar = row![
        button("复制全部")
            .on_press(AppMessage::CopyLogsPressed)
            .style(toolbar_button_style),
        button("导出")
            .on_press(AppMessage::ExportLogsPressed)
            .style(toolbar_button_style),
        button("清空")
            .on_press(AppMessage::ClearLogsPressed)
            .style(toolbar_button_style),
        button("关闭")
            .on_press(AppMessage::CloseLogsWindow)
            .style(toolbar_button_style),
    ]
    .spacing(8);

    let log_text = if logs.is_empty() {
        "暂无日志".to_string()
    } else {
        logs.join("\n")
    };

    let mut content = column![
        text("运行日志")
            .size(22)
            .color(Color::from_rgb8(0xEC, 0xF0, 0xF6)),
        toolbar,
    ]
    .spacing(12);

    let feedback_text = feedback
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(message) = feedback_text {
        content = content.push(
            text(message)
                .size(13)
                .color(Color::from_rgb8(0xA0, 0xA8, 0xB3)),
        );
    }

    content = content.push(
        container(
            scrollable(
                container(
                    text(log_text)
                        .size(12)
                        .line_height(iced::widget::text::LineHeight::Relative(1.45))
                        .font(iced::Font::MONOSPACE)
                        .color(Color::from_rgb8(0xCF, 0xD5, 0xDF)),
                )
                .padding(12),
            )
            .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(Color::from_rgb8(0x1D, 0x22, 0x2B))),
            border: border::rounded(8.0)
                .width(1.0)
                .color(Color::from_rgb8(0x36, 0x3D, 0x48)),
            ..container::Style::default()
        }),
    );

    container(content.padding(16).spacing(12))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(Color::from_rgb8(0x17, 0x1C, 0x24))),
            ..container::Style::default()
        })
        .into()
}

fn toolbar_button_style(theme: &iced::Theme, status: button::Status) -> button::Style {
    let mut style = button::secondary(theme, status);
    style.text_color = Color::from_rgb8(0xE6, 0xEB, 0xF2);
    style
}
