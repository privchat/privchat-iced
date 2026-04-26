use std::collections::HashMap;

use iced::widget::{button, column, container, image, mouse_area, row, scrollable, stack, text, text_input};
use iced::{alignment, border, Background, Color, Element, Length};

use crate::app::message::AppMessage;
use crate::app::state::{AttachmentMenuState, ChatScreenState, DeleteConfirmState};
use crate::presentation::vm::{MessageSendStateVm, PresenceVm};
use crate::ui::icons::{self, Icon};
use crate::ui::widgets::{composer, timeline_list, unread_banner};

const C_HEADER_BG: Color = Color::from_rgb8(0x1A, 0x1D, 0x22);
const C_CHAT_BG: Color = Color::from_rgb8(0x18, 0x1A, 0x1F);
const C_COMPOSER_BG: Color = Color::from_rgb8(0x14, 0x17, 0x1B);
const C_DIVIDER: Color = Color::from_rgb8(0x2A, 0x2E, 0x34);
const C_STATUS_ONLINE: Color = Color::from_rgb8(0x22, 0xC5, 0x5E);
const C_STATUS_OFFLINE: Color = Color::from_rgb8(0x95, 0x9D, 0xA6);
const COMPOSER_HEIGHT: f32 = 184.0;
const EMOJI_POPUP_BOTTOM_OFFSET: f32 = 160.0;

/// Render WeChat-like right chat pane.
pub fn view<'a>(
    chat: &'a ChatScreenState,
    title: &'a str,
    presence: Option<&'a PresenceVm>,
    typing_hint: Option<&'a str>,
    image_cache: &'a HashMap<u64, iced::widget::image::Handle>,
    playing_voice_message_id: Option<u64>,
) -> Element<'a, AppMessage> {
    let title_label: Element<'_, AppMessage> = if let Some(peer_user_id) = chat.peer_user_id {
        mouse_area(
            text(title)
                .size(17)
                .color(Color::from_rgb8(0xF0, 0xF2, 0xF4)),
        )
        .on_press(AppMessage::OpenUserProfile {
            user_id: peer_user_id,
        })
        .interaction(iced::mouse::Interaction::Pointer)
        .into()
    } else {
        text(title)
            .size(17)
            .color(Color::from_rgb8(0xF0, 0xF2, 0xF4))
            .into()
    };

    let header_title = column![title_label, presence_status_text(presence, typing_hint),].spacing(3);
    // 群聊右上角的「群管理」按钮：只在 channel_type==2 时展示，点击打开资料页。
    let group_settings_btn: Option<Element<'_, AppMessage>> = if chat.channel_type == 2 {
        let group_id = chat.channel_id;
        let title = title.to_string();
        Some(
            button(text("群管理").size(12).color(Color::from_rgb8(0xDF, 0x84, 0x1C)))
                .padding([4, 10])
                .on_press(AppMessage::OpenGroupSettings { group_id, title })
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
                        border: border::rounded(4.0),
                        ..button::Style::default()
                    }
                })
                .into(),
        )
    } else {
        None
    };
    let mut header_actions = row![
        icons::render(Icon::BubbleOutline, 26.0, Color::from_rgb8(0xA7, 0xAD, 0xB6)),
        icons::render(Icon::ChevronDown, 17.0, Color::from_rgb8(0xA7, 0xAD, 0xB6)),
        icons::render(Icon::Phone, 26.0, Color::from_rgb8(0xA7, 0xAD, 0xB6)),
    ]
    .spacing(11)
    .align_y(alignment::Vertical::Center);
    if let Some(btn) = group_settings_btn {
        header_actions = header_actions.push(btn);
    }
    header_actions = header_actions.push(icons::render(
        Icon::Dots,
        23.0,
        Color::from_rgb8(0xA7, 0xAD, 0xB6),
    ));

    let header = container(
        row![
            header_title,
            container(header_actions)
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

    let body: Element<'_, AppMessage> = container(
        column![
            unread_banner::view(&chat.unread_marker),
            timeline_list::view(
                chat.channel_id,
                chat.channel_type,
                &chat.timeline,
                image_cache,
                chat.peer_last_read_pts,
                playing_voice_message_id,
                &chat.message_reactions,
                chat.reaction_picker_for,
                chat.open_token,
            ),
        ]
        .height(Length::Fill),
    )
    .height(Length::Fill)
    .style(|_| container::Style {
        background: Some(Background::Color(C_CHAT_BG)),
        ..container::Style::default()
    })
    .into();

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
    } else if chat.composer.quick_phrase_open {
        stack![
            column![header, body, composer]
                .width(Length::Fill)
                .height(Length::Fill),
            mouse_area(container(text("")).width(Length::Fill).height(Length::Fill))
                .on_press(AppMessage::DismissQuickPhrase),
            container(
                column![
                    container(text("")).height(Length::Fill),
                    row![
                        composer::quick_phrase_popup(&chat.composer.quick_phrases, chat.composer.quick_phrase_adding, &chat.composer.quick_phrase_input),
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

    // @ 提及选择器：在群聊 composer 顶部悬浮；筛选为空或 DM 会话时返回 None。
    let content: Element<'_, AppMessage> = if let Some(picker) = chat
        .composer
        .mention_picker
        .as_ref()
        .filter(|_| chat.channel_type != 1)
    {
        if let Some(popup) = composer::mention_picker_popup(picker) {
            stack![
                content,
                container(
                    column![
                        container(text("")).height(Length::Fill),
                        row![
                            popup,
                            container(text("")).width(Length::Fill),
                        ]
                        .width(Length::Fill),
                        container(text("")).height(Length::Fixed(COMPOSER_HEIGHT)),
                    ]
                    .width(Length::Fill)
                    .height(Length::Fill)
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .padding([0, 14]),
            ]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        } else {
            content
        }
    } else {
        content
    };

    let content: Element<'_, AppMessage> = if let Some(pending) = &chat.composer.pending_attachment
    {
        let title = if pending.is_image {
            "发送图片"
        } else {
            "发送文件"
        };
        let preview: Element<'_, AppMessage> = if pending.is_image {
            container(
                image(pending.path.clone())
                    .width(Length::Fixed(300.0))
                    .height(Length::Fixed(180.0))
                    .content_fit(iced::ContentFit::Contain),
            )
            .width(Length::Fill)
            .align_x(alignment::Horizontal::Center)
            .into()
        } else {
            container(text("📎").size(34).color(Color::from_rgb8(0xAF, 0xB6, 0xC1)))
                .width(Length::Fill)
                .align_x(alignment::Horizontal::Center)
                .into()
        };

        stack![
            content,
            mouse_area(
                container(text(""))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(|_| container::Style {
                        background: Some(Background::Color(Color::from_rgba8(0, 0, 0, 0.62))),
                        ..container::Style::default()
                    })
            )
            .on_press(AppMessage::ComposerAttachmentSendCanceled),
            container(
                column![
                    text(title).size(18).color(Color::from_rgb8(0xEA, 0xEE, 0xF3)),
                    preview,
                    text(&pending.filename)
                        .size(14)
                        .color(Color::from_rgb8(0xC1, 0xC8, 0xD2)),
                    row![
                        button(text("取消").size(14))
                            .padding([8, 18])
                            .on_press(AppMessage::ComposerAttachmentSendCanceled),
                        button(text("发送").size(14))
                            .padding([8, 18])
                            .on_press(AppMessage::ComposerAttachmentSendConfirmed),
                    ]
                    .spacing(10)
                ]
                .spacing(12)
                .align_x(alignment::Horizontal::Center)
            )
            .padding([14, 18])
            .width(Length::Fixed(360.0))
            .style(|_| container::Style {
                background: Some(Background::Color(Color::from_rgb8(0x25, 0x2A, 0x31))),
                border: border::width(1.0)
                    .color(Color::from_rgb8(0x3A, 0x41, 0x4B))
                    .rounded(10.0),
                ..container::Style::default()
            })
            .align_x(alignment::Horizontal::Center)
            .align_y(alignment::Vertical::Center)
            .width(Length::Fill)
            .height(Length::Fill)
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else {
        content
    };

    // Right-click context menu overlay (positioned at cursor snapshot).
    // 始终用 stack 包一层：菜单关闭时 stack 只有 base content，打开时再 push 遮罩与菜单弹层。
    // 若按 if-Some/Else 切换 wrapper 类型，scrollable 内部 state 会随 widget tree 重置，
    // 在 anchor_bottom() 下表现为"右键后跳到列表底部"。
    let content: Element<'_, AppMessage> = {
        let mut layers: Vec<Element<'_, AppMessage>> = vec![content];
        if let Some(menu) = &chat.attachment_menu {
            let now_ms = chrono::Utc::now().timestamp_millis();
            let items = build_menu_items(menu, now_ms);
            if !items.is_empty() {
                let pos = menu.anchor_pos.unwrap_or(iced::Point::ORIGIN);
                let offset_x = pos.x.max(0.0);
                let offset_y = pos.y.max(0.0);
                layers.push(
                    mouse_area(
                        container(text(""))
                            .width(Length::Fill)
                            .height(Length::Fill),
                    )
                    .on_press(AppMessage::DismissAttachmentMenu)
                    .on_right_press(AppMessage::DismissAttachmentMenu)
                    .into(),
                );
                layers.push(
                    column![
                        container(text("")).height(Length::Fixed(offset_y)),
                        row![
                            container(text("")).width(Length::Fixed(offset_x)),
                            context_menu_popup(items),
                            container(text("")).width(Length::Fill),
                        ],
                        container(text("")).height(Length::Fill),
                    ]
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into(),
                );
            }
        }
        iced::widget::stack(layers)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    };

    // Delete-message confirmation dialog overlay
    let content: Element<'_, AppMessage> = if let Some(confirm) = &chat.delete_confirm {
        stack![
            content,
            mouse_area(
                container(text(""))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(|_| container::Style {
                        background: Some(Background::Color(Color::from_rgba8(0, 0, 0, 0.5))),
                        ..container::Style::default()
                    })
            )
            .on_press(AppMessage::CancelDeleteMessageLocal),
            container(delete_confirm_card(confirm))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(alignment::Horizontal::Center)
                .align_y(alignment::Vertical::Center)
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else {
        content
    };

    // User profile panel overlay
    let content: Element<'_, AppMessage> =
        if let Some(profile_panel) = &chat.user_profile_panel {
            stack![
                content,
                mouse_area(
                    container(text(""))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .style(|_| container::Style {
                            background: Some(Background::Color(Color::from_rgba8(0, 0, 0, 0.5))),
                            ..container::Style::default()
                        })
                )
                .on_press(AppMessage::CloseUserProfile),
                container(user_profile_card(profile_panel))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(alignment::Horizontal::Center)
                    .align_y(alignment::Vertical::Center)
            ]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        } else {
            content
        };

    mouse_area(
        container(content)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .on_move(AppMessage::ChatCursorMoved)
    .into()
}

const C_CARD_BG: Color = Color::from_rgb8(0x25, 0x2A, 0x31);
const C_CARD_BORDER: Color = Color::from_rgb8(0x3A, 0x41, 0x4B);
const C_CARD_FIELD_LABEL: Color = Color::from_rgb8(0x8B, 0x93, 0x9E);
const C_CARD_FIELD_VALUE: Color = Color::from_rgb8(0xE0, 0xE4, 0xE9);

fn user_profile_card<'a>(
    panel: &'a crate::app::state::UserProfilePanelState,
) -> Element<'a, AppMessage> {
    let content: Element<'_, AppMessage> = if panel.loading {
        container(
            column![
                text("⏳").size(28),
                text("正在加载用户资料...")
                    .size(14)
                    .color(C_CARD_FIELD_LABEL),
            ]
            .spacing(10)
            .align_x(alignment::Horizontal::Center),
        )
        .padding(30)
        .width(Length::Fill)
        .align_x(alignment::Horizontal::Center)
        .into()
    } else if let Some(error) = &panel.error {
        let retry_btn = button(
            text("重试").size(13).color(Color::from_rgb8(0x6B, 0x9F, 0xD2)),
        )
        .padding([6, 16])
        .on_press(AppMessage::OpenUserProfile {
            user_id: panel.user_id,
        })
        .style(|_theme, _status| button::Style {
            background: Some(Background::Color(Color::from_rgb8(0x2F, 0x35, 0x3E))),
            border: border::width(1.0)
                .color(Color::from_rgb8(0x4A, 0x52, 0x5E))
                .rounded(6.0),
            ..button::Style::default()
        });

        container(
            column![
                text("加载失败").size(15).color(Color::from_rgb8(0xEA, 0x5E, 0x5E)),
                text(error).size(12).color(C_CARD_FIELD_LABEL),
                retry_btn,
            ]
            .spacing(10)
            .align_x(alignment::Horizontal::Center),
        )
        .padding(20)
        .width(Length::Fill)
        .align_x(alignment::Horizontal::Center)
        .into()
    } else if let Some(detail) = &panel.detail {
        let title_row: Element<'_, AppMessage> = if panel.editing_alias {
            let input_field = text_input("输入备注名...", &panel.alias_input)
                .on_input(AppMessage::AliasInputChanged)
                .on_submit(AppMessage::ConfirmEditAlias)
                .size(16)
                .padding([4, 8])
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
            let confirm_btn = button(text("确定").size(12).color(Color::WHITE))
                .padding([4, 10])
                .on_press(AppMessage::ConfirmEditAlias)
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
            let cancel_btn = button(text("取消").size(12).color(C_CARD_FIELD_LABEL))
                .padding([4, 10])
                .on_press(AppMessage::CancelEditAlias)
                .style(|_theme, _status| button::Style {
                    background: None,
                    ..button::Style::default()
                });
            column![
                input_field,
                row![cancel_btn, confirm_btn].spacing(8),
            ]
            .spacing(6)
            .into()
        } else {
            row![
                text(&detail.title)
                    .size(18)
                    .color(Color::from_rgb8(0xF0, 0xF2, 0xF4)),
                button(text("修改备注").size(12).color(Color::from_rgb8(0xDF, 0x84, 0x1C)))
                    .padding([2, 8])
                    .on_press(AppMessage::StartEditAlias)
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
                            border: border::rounded(4.0),
                            shadow: Default::default(),
                            snap: true,
                        }
                    }),
            ]
            .spacing(10)
            .align_y(alignment::Vertical::Center)
            .into()
        };

        let mut items = column![
            title_row,
            text(&detail.subtitle)
                .size(13)
                .color(C_CARD_FIELD_LABEL),
        ]
        .spacing(6);

        // Show inline error (e.g. alias set failed)
        if let Some(err) = &panel.error {
            items = items.push(
                text(err)
                    .size(12)
                    .color(Color::from_rgb8(0xEA, 0x5E, 0x5E)),
            );
        }

        // separator
        items = items.push(
            container(text(""))
                .height(Length::Fixed(1.0))
                .width(Length::Fill)
                .style(|_| container::Style {
                    background: Some(Background::Color(C_CARD_BORDER)),
                    ..container::Style::default()
                }),
        );

        for f in &detail.fields {
            let copy_btn = button(
                text("复制").size(11).color(Color::from_rgb8(0x6B, 0x9F, 0xD2)),
            )
            .padding([2, 6])
            .on_press(AppMessage::CopyDetailFieldPressed {
                label: f.label.clone(),
                value: f.value.clone(),
            })
            .style(|_theme, _status| button::Style {
                background: None,
                ..button::Style::default()
            });

            items = items.push(
                row![
                    container(text(&f.label).size(13).color(C_CARD_FIELD_LABEL))
                        .width(Length::Fixed(80.0)),
                    text(&f.value).size(13).color(C_CARD_FIELD_VALUE),
                    copy_btn,
                ]
                .spacing(8)
                .align_y(alignment::Vertical::Center),
            );
        }

        scrollable(
            container(items)
                .padding(20)
                .width(Length::Fill),
        )
        .height(Length::Shrink)
        .into()
    } else {
        container(text("")).into()
    };

    // Wrap in a card with close button
    let close_btn = button(text("✕").size(14).color(C_CARD_FIELD_LABEL))
        .padding([4, 8])
        .on_press(AppMessage::CloseUserProfile)
        .style(|_theme, _status| button::Style {
            background: None,
            ..button::Style::default()
        });

    // 好友设置入口：仅在资料已加载时展示；update 侧接到 OpenFriendSettings
    // 后会解析私聊频道、初始化 mute/block 状态。
    let settings_btn: Option<Element<'_, AppMessage>> =
        panel.detail.as_ref().map(|detail| {
            let title = detail.title.clone();
            let user_id = panel.user_id;
            button(text("好友设置").size(12).color(Color::from_rgb8(0xDF, 0x84, 0x1C)))
                .padding([2, 8])
                .on_press(AppMessage::OpenFriendSettings {
                    user_id,
                    title,
                    avatar: String::new(),
                    remark: String::new(),
                })
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
                        border: border::rounded(4.0),
                        ..button::Style::default()
                    }
                })
                .into()
        });

    let header_actions: Element<'_, AppMessage> = if let Some(btn) = settings_btn {
        row![container(text("")).width(Length::Fill), btn, close_btn]
            .spacing(6)
            .align_y(alignment::Vertical::Center)
            .into()
    } else {
        row![container(text("")).width(Length::Fill), close_btn]
            .align_y(alignment::Vertical::Center)
            .into()
    };

    let card = column![
        container(header_actions)
            .width(Length::Fill)
            .padding(8),
        content,
    ];

    container(card)
        .width(Length::Fixed(340.0))
        .style(|_| container::Style {
            background: Some(Background::Color(C_CARD_BG)),
            border: border::width(1.0)
                .color(C_CARD_BORDER)
                .rounded(12.0),
            ..container::Style::default()
        })
        .into()
}

fn delete_confirm_card<'a>(state: &'a DeleteConfirmState) -> Element<'a, AppMessage> {
    let title = text("删除这条消息？").size(16).color(C_CARD_FIELD_VALUE);
    let hint = text("此操作仅删除本地副本，不会影响其他人。")
        .size(13)
        .color(C_CARD_FIELD_LABEL);

    let mut body = column![title, hint].spacing(10);

    if !state.preview.trim().is_empty() {
        body = body.push(
            container(
                text(state.preview.clone())
                    .size(12)
                    .color(C_CARD_FIELD_LABEL),
            )
            .padding([6, 10])
            .max_width(360.0)
            .style(|_| container::Style {
                background: Some(Background::Color(Color::from_rgba8(0, 0, 0, 0.25))),
                border: border::rounded(6.0),
                ..container::Style::default()
            }),
        );
    }

    let cancel_btn = button(text("取消").size(13).color(C_CARD_FIELD_VALUE))
        .padding([6, 18])
        .on_press(AppMessage::CancelDeleteMessageLocal)
        .style(|_theme, status| {
            let bg = match status {
                button::Status::Hovered | button::Status::Pressed => {
                    Color::from_rgb8(0x3A, 0x41, 0x4B)
                }
                _ => Color::from_rgb8(0x2F, 0x35, 0x3E),
            };
            button::Style {
                background: Some(Background::Color(bg)),
                border: border::width(1.0)
                    .color(Color::from_rgb8(0x4A, 0x52, 0x5E))
                    .rounded(6.0),
                ..button::Style::default()
            }
        });

    let delete_btn = button(text("删除").size(13).color(Color::from_rgb8(0xFF, 0xFF, 0xFF)))
        .padding([6, 18])
        .on_press(AppMessage::DeleteMessageLocalPressed {
            channel_id: state.channel_id,
            channel_type: state.channel_type,
            open_token: state.open_token,
            message_id: state.message_id,
            key: state.message_key.clone(),
        })
        .style(|_theme, status| {
            let bg = match status {
                button::Status::Hovered | button::Status::Pressed => {
                    Color::from_rgb8(0xB9, 0x3A, 0x3A)
                }
                _ => Color::from_rgb8(0xD0, 0x43, 0x43),
            };
            button::Style {
                background: Some(Background::Color(bg)),
                border: border::rounded(6.0),
                ..button::Style::default()
            }
        });

    let actions = row![
        container(text("")).width(Length::Fill),
        cancel_btn,
        delete_btn,
    ]
    .spacing(10);

    container(body.push(actions).spacing(14).padding(20))
        .max_width(420.0)
        .style(|_| container::Style {
            background: Some(Background::Color(C_CARD_BG)),
            border: border::width(1.0)
                .color(C_CARD_BORDER)
                .rounded(12.0),
            ..container::Style::default()
        })
        .into()
}

fn presence_status_text<'a>(
    presence: Option<&'a PresenceVm>,
    typing_hint: Option<&'a str>,
) -> Element<'a, AppMessage> {
    let typing_text = typing_hint.filter(|value| !value.trim().is_empty());
    let presence_status = presence.map(presence_status_bucket);

    match (presence_status, typing_text) {
        (Some((status_label, status_color)), Some(typing_label)) => row![
            text(status_label).size(12).color(status_color),
            text("·").size(12).color(Color::from_rgb8(0x6B, 0x73, 0x7D)),
            text(typing_label).size(12).color(C_STATUS_ONLINE),
        ]
        .spacing(6)
        .align_y(alignment::Vertical::Center)
        .into(),
        (Some((status_label, status_color)), None) => {
            text(status_label).size(12).color(status_color).into()
        }
        (None, Some(typing_label)) => text(typing_label).size(12).color(C_STATUS_ONLINE).into(),
        (None, None) => text("").size(12).color(C_STATUS_OFFLINE).into(),
    }
}

fn build_menu_items(menu: &AttachmentMenuState, now_ms: i64) -> Vec<(String, AppMessage)> {
    let mut items: Vec<(String, AppMessage)> = Vec::new();

    // 本地删除走二次确认；取消发送（pending/sending）是主动放弃发送，不需要确认。
    let request_delete_local = || AppMessage::RequestDeleteMessageLocal {
        channel_id: menu.channel_id,
        channel_type: menu.channel_type,
        open_token: menu.open_token,
        message_id: menu.message_id,
        key: menu.message_key.clone(),
        preview: menu.reply_preview.clone(),
    };
    let cancel_send_local = || AppMessage::DeleteMessageLocalPressed {
        channel_id: menu.channel_id,
        channel_type: menu.channel_type,
        open_token: menu.open_token,
        message_id: menu.message_id,
        key: menu.message_key.clone(),
    };

    if menu.is_revoked {
        items.push(("本地删除".to_string(), request_delete_local()));
        return items;
    }

    let is_pending_or_sending = matches!(
        menu.send_state,
        Some(MessageSendStateVm::Queued)
            | Some(MessageSendStateVm::Sending)
            | Some(MessageSendStateVm::Retrying)
    );
    if is_pending_or_sending {
        items.push(("取消发送".to_string(), cancel_send_local()));
        return items;
    }

    let is_failed = matches!(
        menu.send_state,
        Some(MessageSendStateVm::FailedRetryable { .. })
            | Some(MessageSendStateVm::FailedPermanent { .. })
    );

    if let Some(text_body) = &menu.copy_text {
        if !text_body.is_empty() {
            items.push(("复制".to_string(), AppMessage::TextMenuCopy));
        }
    }

    if menu.is_attachment {
        items.push(("打开".to_string(), AppMessage::AttachmentMenuOpen));
        items.push((
            "打开所在文件夹".to_string(),
            AppMessage::AttachmentMenuOpenFolder,
        ));
        items.push(("另存为".to_string(), AppMessage::AttachmentMenuSaveAs));
    }

    // 引用：仅对已获得 server_message_id 的非失败消息启用（local-first 发送路径需要稳定的目标 id）。
    if !is_failed {
        if let Some(smid) = menu.server_message_id {
            items.push((
                "引用".to_string(),
                AppMessage::ReplyToMessagePressed {
                    server_message_id: smid,
                    from_uid: menu.from_uid,
                    preview: menu.reply_preview.clone(),
                },
            ));
            items.push((
                "反应".to_string(),
                AppMessage::ToggleReactionPicker {
                    message_id: Some(menu.message_id),
                },
            ));
            items.push((
                "转发".to_string(),
                AppMessage::OpenForwardPicker {
                    channel_id: menu.channel_id,
                    channel_type: menu.channel_type,
                    message_id: menu.message_id,
                    server_message_id: Some(smid),
                    preview: menu.reply_preview.clone(),
                },
            ));
        }
    }

    // 撤回：自己发送 + 未失败 + 5 分钟内 + 服务端已确认
    if menu.is_own && !is_failed {
        if let Some(smid) = menu.server_message_id {
            let created_ms = if menu.created_at > 1_000_000_000_000 {
                menu.created_at
            } else {
                menu.created_at.saturating_mul(1000)
            };
            let age_ms = now_ms.saturating_sub(created_ms);
            const RECALL_WINDOW_MS: i64 = 5 * 60 * 1000;
            if (0..=RECALL_WINDOW_MS).contains(&age_ms) {
                items.push((
                    "撤回".to_string(),
                    AppMessage::RevokeMessagePressed {
                        channel_id: menu.channel_id,
                        channel_type: menu.channel_type,
                        server_message_id: smid,
                    },
                ));
            }
        }
    }

    items.push(("本地删除".to_string(), request_delete_local()));
    items
}

fn context_menu_popup<'a>(items: Vec<(String, AppMessage)>) -> Element<'a, AppMessage> {
    let mut col = column![].spacing(1);
    for (label, msg) in items {
        col = col.push(context_menu_item(label, msg));
    }
    container(col)
        .width(Length::Fixed(160.0))
        .padding(4)
        .style(|_| container::Style {
            background: Some(Background::Color(Color::from_rgb8(0x2A, 0x2F, 0x37))),
            border: border::width(1.0)
                .color(Color::from_rgb8(0x3D, 0x44, 0x4D))
                .rounded(8.0),
            ..container::Style::default()
        })
        .into()
}

fn context_menu_item<'a>(label: String, msg: AppMessage) -> Element<'a, AppMessage> {
    button(text(label).size(13).color(Color::from_rgb8(0xE0, 0xE4, 0xEA)))
        .width(Length::Fill)
        .padding([6, 12])
        .style(context_menu_item_style)
        .on_press(msg)
        .into()
}

fn context_menu_item_style(_theme: &iced::Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => {
            Some(Background::Color(Color::from_rgb8(0x36, 0x3C, 0x44)))
        }
        _ => None,
    };
    button::Style {
        background: bg,
        text_color: Color::from_rgb8(0xE0, 0xE4, 0xEA),
        border: border::rounded(4.0),
        shadow: Default::default(),
        snap: true,
    }
}

fn presence_status_bucket(presence: &PresenceVm) -> (String, Color) {
    if presence.is_online {
        return ("在线".to_string(), C_STATUS_ONLINE);
    }

    let last_seen_at = presence.last_seen_at;
    if last_seen_at <= 0 {
        return ("很久没有上线".to_string(), C_STATUS_OFFLINE);
    }

    // last_seen_at is Unix seconds from the server; compare in seconds
    let now = chrono::Utc::now().timestamp();
    let elapsed = now.saturating_sub(last_seen_at);
    let day: i64 = 24 * 60 * 60;

    let label = if elapsed < day {
        "不久前在线"
    } else if elapsed < 7 * day {
        "1天前在线"
    } else if elapsed < 30 * day {
        "7天前在线"
    } else if elapsed < 90 * day {
        "30天前在线"
    } else {
        "很久没有上线"
    };

    (label.to_string(), C_STATUS_OFFLINE)
}
