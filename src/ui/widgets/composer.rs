use iced::widget::{button, column, container, row, text, text_editor};
use iced::{alignment, border, Background, Color, Element, Length, Theme};

use crate::app::message::AppMessage;
use crate::app::state::ComposerState;
use crate::ui::icons::{self, Icon};

/// Render WeChat-like composer: toolbar + input + send button.
pub fn view(composer: &ComposerState) -> Element<'_, AppMessage> {
    let top_line = container(text(""))
        .width(Length::Fill)
        .height(Length::Fixed(1.0))
        .style(|_| container::Style {
            background: Some(Background::Color(Color::from_rgb8(0x2A, 0x2E, 0x34))),
            ..container::Style::default()
        });

    let toolbar = row![
        tool_button(Icon::Smile, Some(AppMessage::ToggleEmojiPicker)),
        tool_button(Icon::Folder, None),
    ]
    .spacing(12)
    .padding([2, 0])
    .align_y(alignment::Vertical::Center);

    let send_enabled = !composer.sending_disabled && !composer.draft.trim().is_empty();
    let editor = text_editor(&composer.editor)
        .placeholder("输入消息")
        .on_action(|action| AppMessage::ComposerEdited { action })
        .padding([8, 0])
        .size(16)
        .height(Length::Fill)
        .style(composer_editor_style);

    let send_button = if send_enabled {
        button(text("发送").size(14))
            .padding([9, 18])
            .style(move |theme, status| send_button_style(theme, status, true))
            .on_press(AppMessage::SendPressed)
    } else {
        button(text("发送").size(14))
            .padding([9, 18])
            .style(move |theme, status| send_button_style(theme, status, false))
    };

    let editor_row = row![
        container(editor).width(Length::Fill).height(Length::Fill),
        container(send_button).align_y(alignment::Vertical::Bottom),
    ]
    .spacing(10)
    .height(Length::Fill)
    .align_y(alignment::Vertical::Bottom);

    composer_shell(
        column![top_line, toolbar, editor_row]
            .spacing(10)
            .padding([8, 14])
            .into(),
    )
}

pub fn emoji_picker_popup() -> Element<'static, AppMessage> {
    emoji_picker()
}

fn composer_shell(content: Element<'_, AppMessage>) -> Element<'_, AppMessage> {
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(Color::from_rgb8(0x14, 0x17, 0x1B))),
            ..container::Style::default()
        })
        .into()
}

fn tool_button(icon: Icon, message: Option<AppMessage>) -> Element<'static, AppMessage> {
    button(icons::render(
        icon,
        24.0,
        Color::from_rgb8(0xA1, 0xA8, 0xB0),
    ))
    .padding([4, 6])
    .on_press_maybe(message)
    .style(|_theme, status| {
        let bg = match status {
            button::Status::Hovered | button::Status::Pressed => Color::from_rgb8(0x2A, 0x2E, 0x35),
            _ => Color::from_rgb8(0x14, 0x17, 0x1B),
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: Color::from_rgb8(0xA1, 0xA8, 0xB0),
            border: border::rounded(6.0),
            shadow: Default::default(),
            snap: true,
        }
    })
    .into()
}

fn emoji_picker() -> Element<'static, AppMessage> {
    let row1 = row![
        emoji_button("😀"),
        emoji_button("😂"),
        emoji_button("😊"),
        emoji_button("😍"),
        emoji_button("🥳"),
        emoji_button("😎"),
    ]
    .spacing(6);

    let row2 = row![
        emoji_button("👍"),
        emoji_button("🙏"),
        emoji_button("🎉"),
        emoji_button("❤️"),
        emoji_button("🔥"),
        emoji_button("✅"),
    ]
    .spacing(6);

    container(column![row1, row2].spacing(6))
        .padding([8, 10])
        .style(|_| container::Style {
            background: Some(Background::Color(Color::from_rgb8(0x22, 0x26, 0x2D))),
            border: border::width(1.0)
                .rounded(8.0)
                .color(Color::from_rgb8(0x3B, 0x41, 0x49)),
            shadow: iced::Shadow {
                color: Color::from_rgba8(0, 0, 0, 0.35),
                offset: iced::Vector::new(0.0, 3.0),
                blur_radius: 10.0,
            },
            ..container::Style::default()
        })
        .into()
}

fn emoji_button(emoji: &'static str) -> Element<'static, AppMessage> {
    button(text(emoji).size(20))
        .padding([4, 6])
        .on_press(AppMessage::EmojiPicked {
            emoji: emoji.to_string(),
        })
        .style(|_theme, status| {
            let bg = match status {
                button::Status::Hovered | button::Status::Pressed => {
                    Color::from_rgb8(0x33, 0x39, 0x42)
                }
                _ => Color::from_rgb8(0x22, 0x26, 0x2D),
            };

            button::Style {
                background: Some(Background::Color(bg)),
                text_color: Color::WHITE,
                border: border::rounded(6.0),
                shadow: Default::default(),
                snap: true,
            }
        })
        .into()
}

fn composer_editor_style(_theme: &Theme, _status: text_editor::Status) -> text_editor::Style {
    text_editor::Style {
        background: Background::Color(Color::from_rgb8(0x14, 0x17, 0x1B)),
        border: border::width(0.0).rounded(0.0).color(Color::TRANSPARENT),
        placeholder: Color::from_rgb8(0x7F, 0x87, 0x91),
        value: Color::from_rgb8(0xE0, 0xE4, 0xEA),
        selection: Color::from_rgb8(0x49, 0x91, 0x6A),
    }
}

fn send_button_style(_theme: &Theme, status: button::Status, enabled: bool) -> button::Style {
    let (bg, fg) = if enabled {
        match status {
            button::Status::Hovered | button::Status::Pressed => (
                Color::from_rgb8(0xC9, 0x72, 0x14),
                Color::from_rgb8(0xFF, 0xFF, 0xFF),
            ),
            _ => (
                Color::from_rgb8(0xDF, 0x84, 0x1C),
                Color::from_rgb8(0xFF, 0xFF, 0xFF),
            ),
        }
    } else {
        (
            Color::from_rgb8(0x3A, 0x40, 0x48),
            Color::from_rgb8(0x8E, 0x96, 0xA0),
        )
    };

    button::Style {
        background: Some(Background::Color(bg)),
        text_color: fg,
        border: border::rounded(8.0),
        shadow: Default::default(),
        snap: true,
    }
}
