use std::collections::HashSet;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use iced::Task;
use tracing::warn;
use uuid::Uuid;

use crate::app::auth_prefs;
use crate::app::message::AppMessage;
use crate::app::route::Route;
use crate::app::state::{
    AppState, ChatScreenState, ComposerState, RuntimeMessageIndex, TimelineState,
};
use crate::presentation::vm::{
    ClientTxnId, MessageSendStateVm, MessageVm, OpenToken, TimelineItemKey, TimelinePatchVm,
    UnreadMarkerVm,
};
use crate::sdk::bridge::SdkBridge;
use crate::sdk::events;

const SIDEBAR_WIDTH: f32 = 70.0;
const PANEL_DIVIDER_WIDTH: f32 = 1.0;
const SESSION_SPLITTER_WIDTH: f32 = 2.0;
const SESSION_SPLITTER_HIT_PADDING: f32 = 8.0;
const SESSION_LIST_MIN_WIDTH: f32 = 260.0;
const SESSION_LIST_MAX_WIDTH: f32 = 620.0;
const CHAT_MIN_WIDTH: f32 = 420.0;
const CHAT_MAX_WIDTH: f32 = 1200.0;

/// Sole mutation entry point.
pub fn update(
    state: &mut AppState,
    message: AppMessage,
    bridge: &Arc<dyn SdkBridge>,
) -> Task<AppMessage> {
    match message {
        AppMessage::Noop => Task::none(),

        AppMessage::StartupRestoreCompleted { session } => {
            if let Some(session) = session {
                apply_login_success(state, session.user_id, session.token, session.device_id);
                return Task::batch([
                    schedule_session_list_refresh(bridge),
                    schedule_total_unread_refresh(bridge),
                ]);
            } else {
                state.route = Route::Login;
                state.auth.is_submitting = false;
            }
            Task::none()
        }

        AppMessage::SessionListLoaded { items } => {
            state.session_list.items = items;
            state.session_list.load_error = None;
            schedule_total_unread_refresh(bridge)
        }

        AppMessage::SessionListLoadFailed { error } => {
            state.session_list.load_error = Some(format_ui_error(&error));
            Task::none()
        }

        AppMessage::TotalUnreadCountLoaded { count } => {
            state.session_list.total_unread_count = count;
            Task::none()
        }

        AppMessage::TotalUnreadCountLoadFailed { error } => {
            state.session_list.load_error =
                Some(format!("UNREAD_COUNT_ERR: {}", format_ui_error(&error)));
            Task::none()
        }

        AppMessage::RefreshTotalUnreadCount => schedule_total_unread_refresh(bridge),

        AppMessage::LoginUsernameChanged { text } => {
            state.auth.username = text;
            Task::none()
        }

        AppMessage::LoginPasswordChanged { text } => {
            state.auth.password = text;
            Task::none()
        }

        AppMessage::LoginDeviceIdChanged { text } => {
            state.auth.device_id = text;
            Task::none()
        }

        AppMessage::FocusNextWidget => {
            if matches!(state.route, Route::Login) {
                iced::widget::operation::focus_next()
            } else {
                Task::none()
            }
        }

        AppMessage::FocusPreviousWidget => {
            if matches!(state.route, Route::Login) {
                iced::widget::operation::focus_previous()
            } else {
                Task::none()
            }
        }

        AppMessage::SessionSplitterDragStarted => {
            state.layout.is_resizing_session_splitter = true;
            Task::none()
        }

        AppMessage::SessionSplitterDragEnded => {
            state.layout.is_resizing_session_splitter = false;
            state.layout.last_cursor_x = None;
            Task::none()
        }

        AppMessage::GlobalLeftMousePressed => {
            if let Some(cursor_x) = state.layout.last_cursor_x {
                if is_cursor_near_session_splitter(state, cursor_x) {
                    state.layout.is_resizing_session_splitter = true;
                }
            }
            Task::none()
        }

        AppMessage::GlobalCursorMoved { x } => {
            state.layout.last_cursor_x = Some(x);

            if !state.layout.is_resizing_session_splitter {
                return Task::none();
            }

            let target = x - SIDEBAR_WIDTH - PANEL_DIVIDER_WIDTH - (SESSION_SPLITTER_WIDTH * 0.5);
            state.layout.session_list_width =
                clamp_session_list_width(state.layout.window_width, target);
            Task::none()
        }

        AppMessage::WindowResized { width } => {
            state.layout.window_width = width;
            state.layout.session_list_width =
                clamp_session_list_width(width, state.layout.session_list_width);
            Task::none()
        }

        AppMessage::LoginPressed => handle_login_submit(state, bridge, false),

        AppMessage::RegisterPressed => handle_login_submit(state, bridge, true),

        AppMessage::LoginSucceeded {
            user_id,
            token,
            device_id,
        } => {
            apply_login_success(state, user_id, token, device_id);
            Task::batch([
                schedule_session_list_refresh(bridge),
                schedule_total_unread_refresh(bridge),
            ])
        }

        AppMessage::LoginFailed { error } => {
            state.auth.is_submitting = false;
            state.auth.error = Some(format_ui_error(&error));
            Task::none()
        }

        AppMessage::ConversationSelected {
            channel_id,
            channel_type,
        } => handle_conversation_selected(state, bridge, channel_id, channel_type),

        AppMessage::ConversationOpened {
            channel_id,
            channel_type,
            open_token,
            snapshot,
        } => {
            if !pass_dual_guard(state, channel_id, channel_type, open_token) {
                return Task::none();
            }
            if let Some(chat) = &mut state.active_chat {
                chat.timeline.revision = snapshot.revision;
                chat.timeline.items = snapshot.items;
                chat.timeline.oldest_server_message_id = snapshot.oldest_server_message_id;
                chat.timeline.has_more_before = snapshot.has_more_before;
                chat.unread_marker = snapshot.unread_marker;
                chat.runtime_index.rebuild_from_items(&chat.timeline.items);
            }
            Task::none()
        }

        AppMessage::ConversationOpenFailed {
            channel_id,
            channel_type,
            open_token,
            error: _,
        } => {
            if !pass_dual_guard(state, channel_id, channel_type, open_token) {
                return Task::none();
            }
            Task::none()
        }

        AppMessage::RetryOpenConversation {
            channel_id,
            channel_type,
        } => handle_conversation_selected(state, bridge, channel_id, channel_type),

        AppMessage::ComposerInputChanged { text } => {
            if let Some(chat) = &mut state.active_chat {
                chat.composer.draft = text;
                chat.composer.editor =
                    iced::widget::text_editor::Content::with_text(&chat.composer.draft);
            }
            Task::none()
        }

        AppMessage::ToggleEmojiPicker => {
            if let Some(chat) = &mut state.active_chat {
                chat.composer.emoji_picker_open = !chat.composer.emoji_picker_open;
            }
            Task::none()
        }

        AppMessage::DismissEmojiPicker => {
            if let Some(chat) = &mut state.active_chat {
                chat.composer.emoji_picker_open = false;
            }
            Task::none()
        }

        AppMessage::EmojiPicked { emoji } => {
            if let Some(chat) = &mut state.active_chat {
                chat.composer.draft.push_str(&emoji);
                chat.composer.editor =
                    iced::widget::text_editor::Content::with_text(&chat.composer.draft);
                chat.composer.emoji_picker_open = false;
            }
            Task::none()
        }

        AppMessage::ComposerEdited { action } => {
            if let Some(chat) = &mut state.active_chat {
                chat.composer.editor.perform(action);
                chat.composer.draft = chat.composer.editor.text();
            }
            Task::none()
        }

        AppMessage::SendPressed => handle_send_pressed(state, bridge),

        AppMessage::RetrySendPressed {
            channel_id,
            channel_type,
            client_txn_id,
        } => handle_retry_send_pressed(state, bridge, channel_id, channel_type, client_txn_id),

        AppMessage::TimelineUpdatedIngress {
            channel_id,
            channel_type,
            open_token,
            message_id,
        } => handle_timeline_updated_ingress(
            state,
            bridge,
            channel_id,
            channel_type,
            open_token,
            message_id,
        ),

        AppMessage::TimelinePatched {
            channel_id,
            channel_type,
            open_token,
            revision,
            patch,
        } => {
            if !pass_dual_guard(state, channel_id, channel_type, open_token) {
                return Task::none();
            }
            if !pass_revision_gate(state, revision) {
                return Task::none();
            }
            let should_refresh_unread = matches!(
                &patch,
                TimelinePatchVm::ReplaceLocalEcho { .. }
                    | TimelinePatchVm::UpsertRemote { .. }
                    | TimelinePatchVm::RemoveMessage { .. }
                    | TimelinePatchVm::UpdateUnreadMarker { .. }
            );
            if let Some(chat) = &mut state.active_chat {
                let applied = apply_timeline_patch(chat, patch);
                if applied {
                    chat.timeline.revision = revision;
                    chat.runtime_index.rebuild_from_items(&chat.timeline.items);
                    if should_refresh_unread {
                        return schedule_total_unread_refresh(bridge);
                    }
                }
            }
            Task::none()
        }

        AppMessage::LoadOlderTriggered {
            channel_id,
            channel_type,
        } => handle_load_older_triggered(state, bridge, channel_id, channel_type),

        AppMessage::HistoryLoaded {
            channel_id,
            channel_type,
            open_token,
            page,
        } => {
            if !pass_dual_guard(state, channel_id, channel_type, open_token) {
                return Task::none();
            }
            if let Some(chat) = &mut state.active_chat {
                chat.timeline.is_loading_more = false;
                chat.timeline.oldest_server_message_id = page.oldest_server_message_id;
                chat.timeline.has_more_before = page.has_more_before;
                prepend_history_items(&mut chat.timeline.items, page.items);
                chat.runtime_index.rebuild_from_items(&chat.timeline.items);
            }
            Task::none()
        }

        AppMessage::HistoryLoadFailed {
            channel_id,
            channel_type,
            open_token,
            error: _,
        } => {
            if !pass_dual_guard(state, channel_id, channel_type, open_token) {
                return Task::none();
            }
            if let Some(chat) = &mut state.active_chat {
                chat.timeline.is_loading_more = false;
            }
            Task::none()
        }

        AppMessage::ViewportChanged {
            channel_id,
            channel_type,
            at_bottom,
            first_visible_item,
        } => handle_viewport_changed(
            state,
            bridge,
            channel_id,
            channel_type,
            at_bottom,
            first_visible_item,
        ),
    }
}

fn handle_conversation_selected(
    state: &mut AppState,
    bridge: &Arc<dyn SdkBridge>,
    channel_id: u64,
    channel_type: i32,
) -> Task<AppMessage> {
    if state.auth.user_id.is_none() {
        state.route = Route::Login;
        return Task::none();
    }

    let open_token = state.allocate_open_token();
    state.route = Route::Chat;
    state.active_chat = Some(ChatScreenState {
        channel_id,
        channel_type,
        open_token,
        timeline: TimelineState::default(),
        runtime_index: RuntimeMessageIndex::default(),
        composer: ComposerState::default(),
        unread_marker: UnreadMarkerVm::default(),
    });

    let bridge = Arc::clone(bridge);
    Task::perform(
        async move { bridge.open_timeline(channel_id, channel_type).await },
        move |result| match result {
            Ok(snapshot) => AppMessage::ConversationOpened {
                channel_id,
                channel_type,
                open_token,
                snapshot,
            },
            Err(error) => AppMessage::ConversationOpenFailed {
                channel_id,
                channel_type,
                open_token,
                error,
            },
        },
    )
}

fn apply_login_success(state: &mut AppState, user_id: u64, token: String, device_id: String) {
    if !state.auth.username.trim().is_empty() {
        auth_prefs::save_last_username(&state.auth.username);
    }
    state.auth.is_submitting = false;
    state.auth.error = None;
    state.auth.user_id = Some(user_id);
    state.auth.token = Some(token);
    state.auth.device_id = device_id;
    state.auth.password.clear();
    state.route = Route::SessionList;
}

fn schedule_session_list_refresh(bridge: &Arc<dyn SdkBridge>) -> Task<AppMessage> {
    let bridge = Arc::clone(bridge);
    Task::perform(
        async move { bridge.load_session_list().await },
        |result| match result {
            Ok(items) => AppMessage::SessionListLoaded { items },
            Err(error) => AppMessage::SessionListLoadFailed { error },
        },
    )
}

fn schedule_total_unread_refresh(bridge: &Arc<dyn SdkBridge>) -> Task<AppMessage> {
    let bridge = Arc::clone(bridge);
    Task::perform(
        async move { bridge.load_total_unread_count(false).await },
        |result| match result {
            Ok(count) => AppMessage::TotalUnreadCountLoaded { count },
            Err(error) => AppMessage::TotalUnreadCountLoadFailed { error },
        },
    )
}

fn handle_login_submit(
    state: &mut AppState,
    bridge: &Arc<dyn SdkBridge>,
    register: bool,
) -> Task<AppMessage> {
    if state.auth.is_submitting {
        return Task::none();
    }

    let username = state.auth.username.trim().to_string();
    let password = state.auth.password.clone();
    let device_id = state.auth.device_id.trim().to_string();

    if username.is_empty() || password.trim().is_empty() || device_id.is_empty() {
        state.auth.error = Some("username/password/device_id are required".to_string());
        return Task::none();
    }
    if Uuid::parse_str(&device_id).is_err() {
        state.auth.error = Some("device_id must be a standard UUID".to_string());
        return Task::none();
    }

    state.auth.is_submitting = true;
    state.auth.error = None;

    let bridge = Arc::clone(bridge);
    Task::perform(
        async move {
            bridge
                .login_with_password(username, password, device_id, register)
                .await
        },
        |result| match result {
            Ok(session) => AppMessage::LoginSucceeded {
                user_id: session.user_id,
                token: session.token,
                device_id: session.device_id,
            },
            Err(error) => AppMessage::LoginFailed { error },
        },
    )
}

fn handle_send_pressed(state: &mut AppState, bridge: &Arc<dyn SdkBridge>) -> Task<AppMessage> {
    let (body, channel_id, channel_type, open_token) = match state.active_chat.as_ref() {
        Some(chat_snapshot) => {
            let body = chat_snapshot.composer.draft.trim().to_string();
            if body.is_empty() {
                return Task::none();
            }
            (
                body,
                chat_snapshot.channel_id,
                chat_snapshot.channel_type,
                chat_snapshot.open_token,
            )
        }
        None => return Task::none(),
    };
    let client_txn_id = state.allocate_client_txn_id();

    if let Some(chat) = &mut state.active_chat {
        let local_echo = MessageVm {
            key: TimelineItemKey::Local(client_txn_id),
            channel_id,
            channel_type,
            message_id: client_txn_id,
            server_message_id: None,
            client_txn_id: Some(client_txn_id),
            from_uid: 0,
            body: body.clone(),
            message_type: 1,
            created_at: now_timestamp_millis(),
            pts: None,
            send_state: Some(MessageSendStateVm::Queued),
            is_own: true,
            is_deleted: false,
        };
        chat.timeline.items.push(local_echo);
        chat.runtime_index.bind(client_txn_id, client_txn_id);
        chat.composer.draft.clear();
        chat.composer.editor = iced::widget::text_editor::Content::new();
        chat.composer.emoji_picker_open = false;
    }

    let bridge = Arc::clone(bridge);
    Task::perform(
        async move {
            bridge
                .send_text_message(channel_id, channel_type, client_txn_id, body)
                .await
        },
        move |result| match result {
            Ok(()) => AppMessage::Noop,
            Err(error) => AppMessage::TimelinePatched {
                channel_id,
                channel_type,
                open_token,
                revision: events::allocate_patch_revision(),
                patch: TimelinePatchVm::UpdateSendState {
                    client_txn_id,
                    send_state: MessageSendStateVm::FailedRetryable { reason: error },
                },
            },
        },
    )
}

fn handle_retry_send_pressed(
    state: &mut AppState,
    bridge: &Arc<dyn SdkBridge>,
    channel_id: u64,
    channel_type: i32,
    client_txn_id: ClientTxnId,
) -> Task<AppMessage> {
    let Some(chat) = &mut state.active_chat else {
        return Task::none();
    };
    if chat.channel_id != channel_id || chat.channel_type != channel_type {
        return Task::none();
    }

    let retry_applied = apply_update_send_state_patch(
        &mut chat.timeline.items,
        client_txn_id,
        MessageSendStateVm::Retrying,
    );
    if retry_applied {
        chat.timeline.revision = events::allocate_patch_revision();
    }

    let open_token = chat.open_token;
    let bridge = Arc::clone(bridge);
    Task::perform(
        async move {
            bridge
                .retry_send(channel_id, channel_type, client_txn_id)
                .await
        },
        move |result| match result {
            Ok(()) => AppMessage::Noop,
            Err(error) => AppMessage::TimelinePatched {
                channel_id,
                channel_type,
                open_token,
                revision: events::allocate_patch_revision(),
                patch: TimelinePatchVm::UpdateSendState {
                    client_txn_id,
                    send_state: MessageSendStateVm::FailedRetryable { reason: error },
                },
            },
        },
    )
}

fn handle_timeline_updated_ingress(
    state: &mut AppState,
    bridge: &Arc<dyn SdkBridge>,
    channel_id: u64,
    channel_type: i32,
    open_token: OpenToken,
    message_id: u64,
) -> Task<AppMessage> {
    if !pass_dual_guard(state, channel_id, channel_type, open_token) {
        return Task::none();
    }

    let replacement_client_txn_id = state
        .active_chat
        .as_ref()
        .and_then(|chat| chat.runtime_index.client_txn_id_for_message(message_id));

    let bridge = Arc::clone(bridge);
    Task::perform(
        async move { bridge.load_message_vm(message_id).await },
        move |result| match result {
            Ok(Some(remote)) => {
                if remote.server_message_id.is_none() {
                    return AppMessage::Noop;
                }
                let patch = match replacement_client_txn_id {
                    Some(client_txn_id) => TimelinePatchVm::ReplaceLocalEcho {
                        client_txn_id,
                        remote,
                    },
                    None => TimelinePatchVm::UpsertRemote { remote },
                };

                AppMessage::TimelinePatched {
                    channel_id,
                    channel_type,
                    open_token,
                    revision: events::allocate_patch_revision(),
                    patch,
                }
            }
            Ok(None) | Err(_) => AppMessage::Noop,
        },
    )
}

fn handle_load_older_triggered(
    state: &mut AppState,
    bridge: &Arc<dyn SdkBridge>,
    channel_id: u64,
    channel_type: i32,
) -> Task<AppMessage> {
    let Some(chat) = &mut state.active_chat else {
        return Task::none();
    };
    if chat.channel_id != channel_id || chat.channel_type != channel_type {
        return Task::none();
    }
    if !chat.timeline.has_more_before || chat.timeline.is_loading_more {
        return Task::none();
    }

    chat.timeline.is_loading_more = true;
    let open_token = chat.open_token;
    let before_server_message_id = chat.timeline.oldest_server_message_id;
    let bridge = Arc::clone(bridge);
    Task::perform(
        async move {
            bridge
                .load_history_before(channel_id, channel_type, before_server_message_id, 50)
                .await
        },
        move |result| match result {
            Ok(page) => AppMessage::HistoryLoaded {
                channel_id,
                channel_type,
                open_token,
                page,
            },
            Err(error) => AppMessage::HistoryLoadFailed {
                channel_id,
                channel_type,
                open_token,
                error,
            },
        },
    )
}

fn handle_viewport_changed(
    state: &mut AppState,
    bridge: &Arc<dyn SdkBridge>,
    channel_id: u64,
    channel_type: i32,
    at_bottom: bool,
    first_visible_item: Option<TimelineItemKey>,
) -> Task<AppMessage> {
    let Some(chat) = &mut state.active_chat else {
        return Task::none();
    };
    if chat.channel_id != channel_id || chat.channel_type != channel_type {
        return Task::none();
    }

    chat.timeline.at_bottom = at_bottom;
    chat.timeline.first_visible_item = first_visible_item;

    if !at_bottom {
        return Task::none();
    }

    let Some(last_read_pts) = latest_read_pts(&chat.timeline.items) else {
        return Task::none();
    };

    let bridge = Arc::clone(bridge);
    Task::perform(
        async move {
            bridge
                .mark_read(channel_id, channel_type, last_read_pts)
                .await
        },
        |result| match result {
            Ok(()) => AppMessage::RefreshTotalUnreadCount,
            Err(_) => AppMessage::Noop,
        },
    )
}

fn clamp_session_list_width(window_width: f32, desired_width: f32) -> f32 {
    let available_main_width =
        (window_width - SIDEBAR_WIDTH - PANEL_DIVIDER_WIDTH - SESSION_SPLITTER_WIDTH).max(0.0);
    let min_by_chat_max = (available_main_width - CHAT_MAX_WIDTH).max(0.0);
    let max_by_chat_min = (available_main_width - CHAT_MIN_WIDTH).max(0.0);

    let mut min_width = SESSION_LIST_MIN_WIDTH.max(min_by_chat_max);
    let max_width = SESSION_LIST_MAX_WIDTH.min(max_by_chat_min);

    if min_width > max_width {
        min_width = max_width;
    }

    desired_width.clamp(min_width, max_width)
}

fn is_cursor_near_session_splitter(state: &AppState, cursor_x: f32) -> bool {
    let splitter_center = SIDEBAR_WIDTH
        + PANEL_DIVIDER_WIDTH
        + state.layout.session_list_width
        + (SESSION_SPLITTER_WIDTH * 0.5);
    (cursor_x - splitter_center).abs() <= SESSION_SPLITTER_HIT_PADDING
}

fn latest_read_pts(items: &[MessageVm]) -> Option<u64> {
    items.iter().rev().find_map(|item| item.pts)
}

fn prepend_history_items(current: &mut Vec<MessageVm>, incoming: Vec<MessageVm>) {
    let mut existing_remote_ids = current
        .iter()
        .filter_map(|item| item.server_message_id)
        .collect::<HashSet<_>>();
    let mut seen_in_batch = HashSet::new();
    let mut deduped = Vec::new();

    for item in incoming {
        let Some(server_message_id) = item.server_message_id else {
            deduped.push(item);
            continue;
        };
        if existing_remote_ids.contains(&server_message_id) {
            continue;
        }
        if !seen_in_batch.insert(server_message_id) {
            continue;
        }
        existing_remote_ids.insert(server_message_id);
        deduped.push(item);
    }

    deduped.append(current);
    *current = deduped;
}

fn apply_timeline_patch(chat: &mut ChatScreenState, patch: TimelinePatchVm) -> bool {
    match patch {
        TimelinePatchVm::ReplaceLocalEcho {
            client_txn_id,
            mut remote,
        } => {
            let Some(server_message_id) = remote.server_message_id else {
                warn!("ignore ReplaceLocalEcho without server_message_id");
                return false;
            };
            remote.client_txn_id = Some(client_txn_id);
            remote.key = TimelineItemKey::Remote { server_message_id };

            if let Some(index) = find_item_index_by_client_txn(&chat.timeline.items, client_txn_id)
            {
                chat.timeline.items[index] = remote;
                dedup_remote_key(&mut chat.timeline.items, server_message_id, index);
                true
            } else if let Some(index) =
                find_item_index_by_server_message_id(&chat.timeline.items, server_message_id)
            {
                chat.timeline.items[index] = remote;
                true
            } else {
                chat.timeline.items.push(remote);
                true
            }
        }
        TimelinePatchVm::UpsertRemote { mut remote } => {
            let Some(server_message_id) = remote.server_message_id else {
                warn!("ignore UpsertRemote without server_message_id");
                return false;
            };
            remote.key = TimelineItemKey::Remote { server_message_id };

            if let Some(index) =
                find_item_index_by_server_message_id(&chat.timeline.items, server_message_id)
            {
                chat.timeline.items[index] = remote;
                true
            } else {
                chat.timeline.items.push(remote);
                true
            }
        }
        TimelinePatchVm::UpdateSendState {
            client_txn_id,
            send_state,
        } => apply_update_send_state_patch(&mut chat.timeline.items, client_txn_id, send_state),
        TimelinePatchVm::RemoveMessage { key } => {
            if let Some(index) = chat.timeline.items.iter().position(|item| item.key == key) {
                if let Some(client_txn_id) = chat.timeline.items[index].client_txn_id {
                    chat.runtime_index.unbind_client_txn_id(client_txn_id);
                }
                chat.timeline.items.remove(index);
                true
            } else {
                false
            }
        }
        TimelinePatchVm::UpdateUnreadMarker { unread_marker } => {
            chat.unread_marker = unread_marker;
            true
        }
    }
}

fn apply_update_send_state_patch(
    items: &mut [MessageVm],
    client_txn_id: ClientTxnId,
    next: MessageSendStateVm,
) -> bool {
    let Some(index) = find_item_index_by_client_txn(items, client_txn_id) else {
        return false;
    };
    let Some(current) = items[index].send_state.clone() else {
        warn!("ignore UpdateSendState for non-own message");
        return false;
    };
    if !is_valid_send_transition(&current, &next) {
        warn!(
            "ignore invalid send state transition: {:?} -> {:?}",
            current, next
        );
        return false;
    }

    items[index].send_state = Some(next.clone());
    if matches!(next, MessageSendStateVm::Sent) {
        if let Some(server_message_id) = items[index].server_message_id {
            items[index].key = TimelineItemKey::Remote { server_message_id };
        }
    }
    true
}

fn is_valid_send_transition(current: &MessageSendStateVm, next: &MessageSendStateVm) -> bool {
    use MessageSendStateVm::{FailedPermanent, FailedRetryable, Queued, Retrying, Sending, Sent};

    if current == next {
        return true;
    }
    if matches!(current, Sent | FailedPermanent { .. }) {
        return false;
    }

    matches!(
        (current, next),
        (Queued, Sending)
            | (Queued, FailedRetryable { .. })
            | (Queued, FailedPermanent { .. })
            | (Sending, Sent)
            | (Sending, FailedRetryable { .. })
            | (Sending, FailedPermanent { .. })
            | (Sending, Retrying)
            | (FailedRetryable { .. }, Retrying)
            | (Retrying, Sending)
            | (Retrying, FailedRetryable { .. })
            | (Retrying, FailedPermanent { .. })
            | (Retrying, Sent)
    )
}

fn find_item_index_by_client_txn(items: &[MessageVm], client_txn_id: ClientTxnId) -> Option<usize> {
    items.iter().position(|item| {
        item.client_txn_id == Some(client_txn_id)
            || matches!(item.key, TimelineItemKey::Local(id) if id == client_txn_id)
    })
}

fn find_item_index_by_server_message_id(
    items: &[MessageVm],
    server_message_id: u64,
) -> Option<usize> {
    items
        .iter()
        .position(|item| item.server_message_id == Some(server_message_id))
}

fn dedup_remote_key(items: &mut Vec<MessageVm>, server_message_id: u64, keep_index: usize) {
    let mut index = 0usize;
    items.retain(|item| {
        let should_keep = item.server_message_id != Some(server_message_id) || index == keep_index;
        index = index.saturating_add(1);
        should_keep
    });
}

fn pass_dual_guard(
    state: &AppState,
    channel_id: u64,
    channel_type: i32,
    open_token: OpenToken,
) -> bool {
    match &state.active_chat {
        Some(chat) => {
            chat.channel_id == channel_id
                && chat.channel_type == channel_type
                && chat.open_token == open_token
        }
        None => false,
    }
}

fn pass_revision_gate(state: &AppState, revision: u64) -> bool {
    match &state.active_chat {
        Some(chat) => revision > chat.timeline.revision,
        None => false,
    }
}

fn now_timestamp_millis() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis() as i64,
        Err(_) => 0,
    }
}

fn format_ui_error(error: &crate::presentation::vm::UiError) -> String {
    match error {
        crate::presentation::vm::UiError::Unknown(message) => message.clone(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use iced::Subscription;
    use privchat_sdk::SdkEvent;

    use super::update;
    use crate::app::message::AppMessage;
    use crate::app::route::Route;
    use crate::app::state::{
        AppState, ChatScreenState, ComposerState, RuntimeMessageIndex, TimelineState,
    };
    use crate::presentation::vm::{
        MessageSendStateVm, MessageVm, TimelineItemKey, TimelinePatchVm, UiError, UnreadMarkerVm,
    };
    use crate::sdk::bridge::SdkBridge;

    #[derive(Clone, Default)]
    struct StubBridge;

    #[async_trait]
    impl SdkBridge for StubBridge {
        async fn restore_session(
            &self,
        ) -> Result<Option<crate::presentation::vm::LoginSessionVm>, UiError> {
            Ok(None)
        }

        async fn load_session_list(
            &self,
        ) -> Result<Vec<crate::presentation::vm::SessionListItemVm>, UiError> {
            Ok(Vec::new())
        }

        async fn load_total_unread_count(&self, _exclude_muted: bool) -> Result<u32, UiError> {
            Ok(0)
        }

        async fn login_with_password(
            &self,
            _username: String,
            _password: String,
            _device_id: String,
            _register: bool,
        ) -> Result<crate::presentation::vm::LoginSessionVm, UiError> {
            Err(UiError::Unknown("unused".to_string()))
        }

        async fn open_timeline(
            &self,
            _channel_id: u64,
            _channel_type: i32,
        ) -> Result<crate::presentation::vm::TimelineSnapshotVm, UiError> {
            Err(UiError::Unknown("unused".to_string()))
        }

        async fn send_text_message(
            &self,
            _channel_id: u64,
            _channel_type: i32,
            _client_txn_id: u64,
            _body: String,
        ) -> Result<(), UiError> {
            Err(UiError::Unknown("unused".to_string()))
        }

        async fn retry_send(
            &self,
            _channel_id: u64,
            _channel_type: i32,
            _client_txn_id: u64,
        ) -> Result<(), UiError> {
            Err(UiError::Unknown("unused".to_string()))
        }

        async fn load_history_before(
            &self,
            _channel_id: u64,
            _channel_type: i32,
            _before_server_message_id: Option<u64>,
            _limit: usize,
        ) -> Result<crate::presentation::vm::HistoryPageVm, UiError> {
            Err(UiError::Unknown("unused".to_string()))
        }

        async fn load_message_vm(&self, _message_id: u64) -> Result<Option<MessageVm>, UiError> {
            Ok(None)
        }

        async fn mark_read(
            &self,
            _channel_id: u64,
            _channel_type: i32,
            _last_read_pts: u64,
        ) -> Result<(), UiError> {
            Ok(())
        }

        fn subscribe_timeline(&self) -> Subscription<SdkEvent> {
            Subscription::none()
        }
    }

    fn base_state() -> AppState {
        let mut state = AppState::new();
        state.route = Route::Chat;
        state.active_chat = Some(ChatScreenState {
            channel_id: 100,
            channel_type: 2,
            open_token: 1,
            timeline: TimelineState::default(),
            runtime_index: RuntimeMessageIndex::default(),
            composer: ComposerState::default(),
            unread_marker: UnreadMarkerVm::default(),
        });
        state
    }

    fn own_local_message(client_txn_id: u64) -> MessageVm {
        MessageVm {
            key: TimelineItemKey::Local(client_txn_id),
            channel_id: 100,
            channel_type: 2,
            message_id: client_txn_id,
            server_message_id: None,
            client_txn_id: Some(client_txn_id),
            from_uid: 7,
            body: "hello".to_string(),
            message_type: 1,
            created_at: 0,
            pts: None,
            send_state: Some(MessageSendStateVm::Queued),
            is_own: true,
            is_deleted: false,
        }
    }

    #[test]
    fn patch_for_non_current_channel_is_blocked_by_dual_guard() {
        let mut state = base_state();
        let bridge: Arc<dyn SdkBridge> = Arc::new(StubBridge);

        let _ = update(
            &mut state,
            AppMessage::TimelinePatched {
                channel_id: 999,
                channel_type: 2,
                open_token: 1,
                revision: 1,
                patch: TimelinePatchVm::UpdateUnreadMarker {
                    unread_marker: UnreadMarkerVm {
                        first_unread_key: None,
                        unread_count: 10,
                        has_unread_below_viewport: true,
                    },
                },
            },
            &bridge,
        );

        let unread_count = state
            .active_chat
            .as_ref()
            .expect("chat")
            .unread_marker
            .unread_count;
        assert_eq!(unread_count, 0);
    }

    #[test]
    fn equal_or_older_revision_patch_is_ignored() {
        let mut state = base_state();
        let bridge: Arc<dyn SdkBridge> = Arc::new(StubBridge);

        if let Some(chat) = &mut state.active_chat {
            chat.timeline.revision = 5;
            chat.timeline.items.push(own_local_message(2001));
            chat.runtime_index.bind(2001, 2001);
        }

        let _ = update(
            &mut state,
            AppMessage::TimelinePatched {
                channel_id: 100,
                channel_type: 2,
                open_token: 1,
                revision: 5,
                patch: TimelinePatchVm::UpdateSendState {
                    client_txn_id: 2001,
                    send_state: MessageSendStateVm::Sent,
                },
            },
            &bridge,
        );

        let chat = state.active_chat.as_ref().expect("chat");
        assert_eq!(chat.timeline.revision, 5);
        assert!(matches!(
            chat.timeline.items[0].send_state,
            Some(MessageSendStateVm::Queued)
        ));
    }

    #[test]
    fn update_send_state_uses_client_txn_identity() {
        let mut state = base_state();
        let bridge: Arc<dyn SdkBridge> = Arc::new(StubBridge);

        if let Some(chat) = &mut state.active_chat {
            chat.timeline.items.push(own_local_message(2001));
            chat.runtime_index.bind(2001, 2001);
        }

        let _ = update(
            &mut state,
            AppMessage::TimelinePatched {
                channel_id: 100,
                channel_type: 2,
                open_token: 1,
                revision: 1,
                patch: TimelinePatchVm::UpdateSendState {
                    client_txn_id: 9999,
                    send_state: MessageSendStateVm::Sent,
                },
            },
            &bridge,
        );

        let send_state = state.active_chat.as_ref().expect("chat").timeline.items[0]
            .send_state
            .clone();
        assert!(matches!(send_state, Some(MessageSendStateVm::Queued)));
    }

    #[test]
    fn upsert_remote_without_server_message_id_is_ignored() {
        let mut state = base_state();
        let bridge: Arc<dyn SdkBridge> = Arc::new(StubBridge);

        let remote_without_server_id = MessageVm {
            key: TimelineItemKey::Local(0),
            channel_id: 100,
            channel_type: 2,
            message_id: 321,
            server_message_id: None,
            client_txn_id: None,
            from_uid: 42,
            body: "remote".to_string(),
            message_type: 1,
            created_at: 0,
            pts: Some(99),
            send_state: None,
            is_own: false,
            is_deleted: false,
        };

        let _ = update(
            &mut state,
            AppMessage::TimelinePatched {
                channel_id: 100,
                channel_type: 2,
                open_token: 1,
                revision: 1,
                patch: TimelinePatchVm::UpsertRemote {
                    remote: remote_without_server_id,
                },
            },
            &bridge,
        );

        let chat = state.active_chat.as_ref().expect("chat");
        assert!(chat.timeline.items.is_empty());
        assert_eq!(chat.timeline.revision, 0);
    }

    #[test]
    fn upsert_remote_for_current_chat_enters_timeline() {
        let mut state = base_state();
        let bridge: Arc<dyn SdkBridge> = Arc::new(StubBridge);

        let remote = MessageVm {
            key: TimelineItemKey::Remote {
                server_message_id: 888,
            },
            channel_id: 100,
            channel_type: 2,
            message_id: 5001,
            server_message_id: Some(888),
            client_txn_id: None,
            from_uid: 42,
            body: "remote".to_string(),
            message_type: 1,
            created_at: 1,
            pts: Some(22),
            send_state: None,
            is_own: false,
            is_deleted: false,
        };

        let _ = update(
            &mut state,
            AppMessage::TimelinePatched {
                channel_id: 100,
                channel_type: 2,
                open_token: 1,
                revision: 1,
                patch: TimelinePatchVm::UpsertRemote { remote },
            },
            &bridge,
        );

        let chat = state.active_chat.as_ref().expect("chat");
        assert_eq!(chat.timeline.items.len(), 1);
        assert_eq!(chat.timeline.items[0].server_message_id, Some(888));
        assert_eq!(chat.timeline.revision, 1);
    }

    #[test]
    fn login_success_moves_route_to_session_list() {
        let mut state = AppState::new();
        let bridge: Arc<dyn SdkBridge> = Arc::new(StubBridge);
        state.route = Route::Login;

        let _ = update(
            &mut state,
            AppMessage::LoginSucceeded {
                user_id: 42,
                token: "token-1".to_string(),
                device_id: "dev-1".to_string(),
            },
            &bridge,
        );

        assert!(matches!(state.route, Route::SessionList));
        assert_eq!(state.auth.user_id, Some(42));
        assert_eq!(state.auth.token.as_deref(), Some("token-1"));
    }
}
