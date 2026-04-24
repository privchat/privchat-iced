use iced::widget::{button, column, container, mouse_area, row, scrollable, text, text_editor, text_input};
use iced::{alignment, border, Background, Color, Element, Length, Theme};

use crate::app::message::AppMessage;
use crate::app::state::{ComposerState, MentionPickerState};
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
        tool_button(Icon::QuickPhrase, Some(AppMessage::ToggleQuickPhrase)),
        tool_button(Icon::Image, Some(AppMessage::ComposerPickImagePressed)),
        tool_button(Icon::Folder, Some(AppMessage::ComposerPickFilePressed)),
    ]
    .spacing(12)
    .padding([2, 0])
    .align_y(alignment::Vertical::Center);

    let reply_banner: Option<Element<'_, AppMessage>> =
        composer.pending_reply.as_ref().map(|reply| {
            let raw = reply.preview.replace('\n', " ");
            let preview = if raw.chars().count() > 48 {
                let truncated: String = raw.chars().take(48).collect();
                format!("{}…", truncated)
            } else {
                raw
            };
            container(
                row![
                    container(text(""))
                        .width(Length::Fixed(3.0))
                        .height(Length::Fixed(22.0))
                        .style(|_| container::Style {
                            background: Some(Background::Color(Color::from_rgb8(
                                0xDF, 0x84, 0x1C,
                            ))),
                            ..container::Style::default()
                        }),
                    text(format!("引用: {}", preview))
                        .size(12)
                        .color(Color::from_rgb8(0xC1, 0xC8, 0xD2)),
                    container(text(""))
                        .width(Length::Fill),
                    button(
                        text("\u{00D7}")
                            .size(14)
                            .color(Color::from_rgb8(0x8E, 0x96, 0xA0)),
                    )
                    .padding([0, 8])
                    .on_press(AppMessage::CancelPendingReply)
                    .style(|_theme, status| {
                        let bg = match status {
                            button::Status::Hovered | button::Status::Pressed => {
                                Color::from_rgb8(0x2A, 0x2E, 0x35)
                            }
                            _ => Color::TRANSPARENT,
                        };
                        button::Style {
                            background: Some(Background::Color(bg)),
                            text_color: Color::from_rgb8(0x8E, 0x96, 0xA0),
                            border: border::rounded(4.0),
                            shadow: Default::default(),
                            snap: true,
                        }
                    }),
                ]
                .spacing(8)
                .align_y(alignment::Vertical::Center),
            )
            .padding([6, 10])
            .width(Length::Fill)
            .style(|_| container::Style {
                background: Some(Background::Color(Color::from_rgb8(0x1C, 0x20, 0x26))),
                border: border::width(1.0)
                    .color(Color::from_rgb8(0x2E, 0x33, 0x3A))
                    .rounded(6.0),
                ..container::Style::default()
            })
            .into()
        });

    let send_enabled = !composer.sending_disabled && !composer.draft.trim().is_empty();
    let editor = text_editor(&composer.editor)
        .id("chat-composer-editor")
        .placeholder("输入消息")
        .key_binding(|key_press| {
            use iced::keyboard::{key, Key};
            if matches!(key_press.key.as_ref(), Key::Named(key::Named::Enter)) {
                if key_press.modifiers.shift() {
                    Some(text_editor::Binding::Enter)
                } else {
                    Some(text_editor::Binding::Custom(AppMessage::SendPressed))
                }
            } else if matches!(key_press.key.as_ref(), Key::Character("v"))
                && (key_press.modifiers.command() || key_press.modifiers.control())
                && !key_press.modifiers.alt()
            {
                Some(text_editor::Binding::Custom(AppMessage::ComposerPastePressed))
            } else {
                text_editor::Binding::from_key_press(key_press)
            }
        })
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

    let mut layout = column![top_line, toolbar].spacing(10);
    if let Some(banner) = reply_banner {
        layout = layout.push(banner);
    }
    layout = layout.push(editor_row);

    composer_shell(layout.padding([8, 14]).into())
}

pub fn emoji_picker_popup() -> Element<'static, AppMessage> {
    emoji_picker()
}

/// @ 提及选择器：锚定在输入框上方，无候选时返回 None（调用方不渲染）。
pub fn mention_picker_popup<'a>(state: &'a MentionPickerState) -> Option<Element<'a, AppMessage>> {
    if state.filtered.is_empty() {
        return None;
    }

    let mut items: Vec<Element<'a, AppMessage>> = Vec::new();
    for member in &state.filtered {
        let label = {
            let l = member.best_label();
            if l.is_empty() {
                format!("用户{}", member.user_id)
            } else {
                l.to_string()
            }
        };
        let row_content = container(
            text(label)
                .size(14)
                .color(Color::from_rgb8(0xE0, 0xE4, 0xEA)),
        )
        .padding([8, 12])
        .width(Length::Fill);
        let user_id = member.user_id;
        let pressable = button(row_content)
            .padding(0)
            .width(Length::Fill)
            .on_press(AppMessage::MentionPickerPicked { user_id })
            .style(|_theme, status| {
                let bg = match status {
                    button::Status::Hovered | button::Status::Pressed => {
                        Color::from_rgb8(0x2A, 0x2E, 0x35)
                    }
                    _ => Color::TRANSPARENT,
                };
                button::Style {
                    background: Some(Background::Color(bg)),
                    text_color: Color::from_rgb8(0xE0, 0xE4, 0xEA),
                    border: border::rounded(0.0),
                    shadow: Default::default(),
                    snap: true,
                }
            });
        items.push(pressable.into());
    }

    let list = scrollable(column(items).width(Length::Fill));
    let popup = container(list)
        .width(Length::Fixed(280.0))
        .max_height(220.0)
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
        });
    Some(popup.into())
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

pub fn quick_phrase_popup<'a>(phrases: &'a [String], adding: bool, input: &'a str) -> Element<'a, AppMessage> {
    let mut items: Vec<Element<'_, AppMessage>> = Vec::new();

    for (i, phrase) in phrases.iter().enumerate() {
        let label = if phrase.chars().count() > 20 {
            let truncated: String = phrase.chars().take(20).collect();
            format!("{}...", truncated)
        } else {
            phrase.clone()
        };
        let phrase_row = mouse_area(
            container(
                row![
                    container(text(label).size(14).color(Color::from_rgb8(0xE0, 0xE4, 0xEA)))
                        .width(Length::Fill),
                    button(text("\u{00D7}").size(14).color(Color::from_rgb8(0x8E, 0x96, 0xA0)))
                        .padding([2, 6])
                        .on_press(AppMessage::QuickPhraseDelete { index: i })
                        .style(|_theme, status| {
                            let bg = match status {
                                button::Status::Hovered | button::Status::Pressed => {
                                    Color::from_rgb8(0x3A, 0x40, 0x48)
                                }
                                _ => Color::TRANSPARENT,
                            };
                            button::Style {
                                background: Some(Background::Color(bg)),
                                text_color: Color::from_rgb8(0x8E, 0x96, 0xA0),
                                border: border::rounded(4.0),
                                shadow: Default::default(),
                                snap: true,
                            }
                        }),
                ]
                .spacing(4)
                .align_y(alignment::Vertical::Center),
            )
            .padding([8, 12])
            .width(Length::Fill)
            .style(|_| container::Style::default()),
        )
        .on_press(AppMessage::QuickPhrasePicked { index: i });

        items.push(phrase_row.into());
    }

    if adding {
        // Inline input row for adding a new phrase
        let input_field = text_input("输入常用消息...", input)
            .on_input(AppMessage::QuickPhraseInputChanged)
            .on_submit(AppMessage::QuickPhraseConfirmAdd)
            .size(14)
            .padding([6, 8])
            .style(|_theme, _status| text_input::Style {
                background: Background::Color(Color::from_rgb8(0x1A, 0x1E, 0x24)),
                border: border::width(1.0)
                    .rounded(6.0)
                    .color(Color::from_rgb8(0x3B, 0x41, 0x49)),
                icon: Color::from_rgb8(0x8E, 0x96, 0xA0),
                placeholder: Color::from_rgb8(0x7F, 0x87, 0x91),
                value: Color::from_rgb8(0xE0, 0xE4, 0xEA),
                selection: Color::from_rgb8(0x49, 0x91, 0x6A),
            });

        let confirm_btn = button(text("确定").size(13).color(Color::WHITE))
            .padding([5, 12])
            .on_press(AppMessage::QuickPhraseConfirmAdd)
            .style(|_theme, status| {
                let bg = match status {
                    button::Status::Hovered | button::Status::Pressed => {
                        Color::from_rgb8(0xC9, 0x72, 0x14)
                    }
                    _ => Color::from_rgb8(0xDF, 0x84, 0x1C),
                };
                button::Style {
                    background: Some(Background::Color(bg)),
                    text_color: Color::WHITE,
                    border: border::rounded(6.0),
                    shadow: Default::default(),
                    snap: true,
                }
            });

        let cancel_btn = button(text("取消").size(13).color(Color::from_rgb8(0x8E, 0x96, 0xA0)))
            .padding([5, 12])
            .on_press(AppMessage::QuickPhraseCancelAdd)
            .style(|_theme, status| {
                let bg = match status {
                    button::Status::Hovered | button::Status::Pressed => {
                        Color::from_rgb8(0x2A, 0x2E, 0x35)
                    }
                    _ => Color::TRANSPARENT,
                };
                button::Style {
                    background: Some(Background::Color(bg)),
                    text_color: Color::from_rgb8(0x8E, 0x96, 0xA0),
                    border: border::rounded(6.0),
                    shadow: Default::default(),
                    snap: true,
                }
            });

        let input_row = container(
            column![
                input_field,
                row![cancel_btn, confirm_btn]
                    .spacing(8)
                    .align_y(alignment::Vertical::Center),
            ]
            .spacing(8),
        )
        .padding([8, 12])
        .width(Length::Fill);

        items.push(input_row.into());
    } else {
        // "添加常用消息" button at the bottom
        let add_button = button(
            row![
                icons::render(Icon::Plus, 16.0, Color::from_rgb8(0xDF, 0x84, 0x1C)),
                text("添加常用消息").size(14).color(Color::from_rgb8(0xDF, 0x84, 0x1C)),
            ]
            .spacing(6)
            .align_y(alignment::Vertical::Center),
        )
        .padding([8, 12])
        .width(Length::Fill)
        .on_press(AppMessage::OpenAddQuickPhrase)
        .style(|_theme, status| {
            let bg = match status {
                button::Status::Hovered | button::Status::Pressed => {
                    Color::from_rgb8(0x2A, 0x2E, 0x35)
                }
                _ => Color::TRANSPARENT,
            };
            button::Style {
                background: Some(Background::Color(bg)),
                text_color: Color::from_rgb8(0xDF, 0x84, 0x1C),
                border: border::rounded(0.0),
                shadow: Default::default(),
                snap: true,
            }
        });
        items.push(add_button.into());
    }

    let list = scrollable(column(items).width(Length::Fill))
        .height(Length::Shrink);

    container(list)
        .width(Length::Fixed(320.0))
        .max_height(300.0)
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
