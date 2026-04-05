use iced::widget::{column, container, mouse_area, row, stack, text};
use iced::{alignment, border, Background, Color, Element, Length};

use crate::app::message::AppMessage;
use crate::app::state::ChatScreenState;
use crate::ui::icons::{self, Icon};
use crate::ui::widgets::{composer, timeline_list, unread_banner};

const C_HEADER_BG: Color = Color::from_rgb8(0x1A, 0x1D, 0x22);
const C_CHAT_BG: Color = Color::from_rgb8(0x18, 0x1A, 0x1F);
const C_COMPOSER_BG: Color = Color::from_rgb8(0x14, 0x17, 0x1B);
const C_DIVIDER: Color = Color::from_rgb8(0x2A, 0x2E, 0x34);
const COMPOSER_HEIGHT: f32 = 184.0;
const EMOJI_POPUP_BOTTOM_OFFSET: f32 = 160.0;

/// Render WeChat-like right chat pane.
pub fn view<'a>(chat: &'a ChatScreenState, title: &'a str) -> Element<'a, AppMessage> {
    let header = container(
        row![
            text(title)
                .size(17)
                .color(Color::from_rgb8(0xF0, 0xF2, 0xF4)),
            container(
                row![
                    icons::render(
                        Icon::BubbleOutline,
                        26.0,
                        Color::from_rgb8(0xA7, 0xAD, 0xB6)
                    ),
                    icons::render(Icon::ChevronDown, 17.0, Color::from_rgb8(0xA7, 0xAD, 0xB6)),
                    icons::render(Icon::Phone, 26.0, Color::from_rgb8(0xA7, 0xAD, 0xB6)),
                    icons::render(Icon::Dots, 23.0, Color::from_rgb8(0xA7, 0xAD, 0xB6)),
                ]
                .spacing(11)
                .align_y(alignment::Vertical::Center),
            )
            .width(Length::Fill)
            .align_x(alignment::Horizontal::Right)
            .align_y(alignment::Vertical::Center)
        ]
        .height(Length::Fill)
        .align_y(alignment::Vertical::Center),
    )
    .height(Length::Fixed(58.0))
    .padding([0, 16])
    .style(|_| container::Style {
        background: Some(Background::Color(C_HEADER_BG)),
        border: border::width(1.0).color(C_DIVIDER),
        ..container::Style::default()
    });

    let body = container(
        column![
            unread_banner::view(&chat.unread_marker),
            timeline_list::view(chat.channel_id, chat.channel_type, &chat.timeline),
        ]
        .height(Length::Fill),
    )
    .height(Length::Fill)
    .style(|_| container::Style {
        background: Some(Background::Color(C_CHAT_BG)),
        ..container::Style::default()
    });

    let composer = container(composer::view(&chat.composer))
        .height(Length::Fixed(COMPOSER_HEIGHT))
        .style(|_| container::Style {
            background: Some(Background::Color(C_COMPOSER_BG)),
            border: border::width(1.0).color(C_DIVIDER),
            ..container::Style::default()
        });

    let content: Element<'_, AppMessage> = if chat.composer.emoji_picker_open {
        stack![
            column![header, body, composer]
                .width(Length::Fill)
                .height(Length::Fill),
            mouse_area(container(text("")).width(Length::Fill).height(Length::Fill))
                .on_press(AppMessage::DismissEmojiPicker),
            container(
                column![
                    container(text("")).height(Length::Fill),
                    row![
                        composer::emoji_picker_popup(),
                        container(text("")).width(Length::Fill)
                    ]
                    .width(Length::Fill),
                    container(text("")).height(Length::Fixed(EMOJI_POPUP_BOTTOM_OFFSET))
                ]
                .width(Length::Fill)
                .height(Length::Fill)
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .padding([0, 14])
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else {
        column![header, body, composer]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    };

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
