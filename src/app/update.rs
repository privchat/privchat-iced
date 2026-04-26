use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use iced::{window, Size, Task};
use privchat_protocol::message::ContentMessageType;
use tokio::time::{sleep, Duration};
use tracing::warn;
use uuid::Uuid;

use crate::app::auth_prefs;
use crate::app::message::{AppMessage, MessageIngressSource};
use crate::app::reporting::{self, TimelinePatchKind};
use crate::app::route::Route;
use crate::app::state::{
    AppState, ChatScreenState, ComposerState, ForwardPickerState, FriendSettingsState,
    GroupSettingsState, PendingAttachmentState, RuntimeMessageIndex, TimelineState,
};
use crate::audio;
use crate::presentation::vm::{
    AddFriendSelectionVm, ClientTxnId, ForwardSendSummary, ForwardTarget, MessageSendStateVm,
    MessageVm, OpenToken, TimelineItemKey, TimelinePatchVm, UiError, UnreadMarkerVm,
    FORWARD_MAX_TARGETS, FORWARD_NOTE_MAX,
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
const TEXT_MESSAGE_TYPE: i32 = ContentMessageType::Text as i32;
const IMAGE_MESSAGE_TYPE: i32 = ContentMessageType::Image as i32;
const FILE_MESSAGE_TYPE: i32 = ContentMessageType::File as i32;
const VIDEO_MESSAGE_TYPE: i32 = ContentMessageType::Video as i32;
const MAX_RUNTIME_LOGS: usize = 1200;
const TYPING_HINT_TTL_MILLIS: u64 = 4_000;

/// Sole mutation entry point.
pub fn update(
    state: &mut AppState,
    message: AppMessage,
    bridge: &Arc<dyn SdkBridge>,
) -> Task<AppMessage> {
    append_runtime_log(
        state,
        "EVENT",
        &truncate_log_line(&format!("{message:?}"), 240),
    );
    // NOTE: Read Gate v1 - active_read_channel_id is controlled explicitly by enter/leave.

    match message {
        AppMessage::Noop => Task::none(),

        AppMessage::StartupRestoreCompleted { session } => {
            if let Some(session) = session {
                apply_login_success(state, session.user_id, session.token, session.device_id);
                return Task::batch([
                    schedule_session_list_refresh(state, bridge),
                    schedule_total_unread_refresh(bridge),
                    schedule_local_accounts_refresh(bridge),
                ]);
            } else {
                state.route = Route::Login;
                state.auth.is_submitting = false;
                state.switch_account.add_account_login_mode = false;
                // Ensure read gate is cleared on login failure / logout
                leave_reading_conversation(state);
            }
            Task::none()
        }

        AppMessage::SessionListLoaded { items } => {
            state.session_list.is_loading = false;
            state.session_list.items = items;
            state.session_list.load_error = None;
            if let Some(chat) = &mut state.active_chat {
                if let Some(item) = state.session_list.items.iter().find(|item| {
                    item.channel_id == chat.channel_id && item.channel_type == chat.channel_type
                }) {
                    if !item.title.trim().is_empty() {
                        chat.title = item.title.clone();
                    }
                    // Always overwrite to avoid carrying stale peer from previous conversation.
                    chat.peer_user_id = item.peer_user_id;
                }
            }
            // Read Gate: the currently viewed conversation must never show unread
            // badges or contribute to the total badge. Server snapshot can still
            // carry a stale positive count because mark_read is eventually consistent.
            if let Some(active_channel_id) = state.active_read_channel_id {
                for item in state.session_list.items.iter_mut() {
                    if item.channel_id == active_channel_id {
                        item.unread_count = 0;
                    }
                }
            }
            state.session_list.total_unread_count = state
                .session_list
                .items
                .iter()
                .map(|item| item.unread_count)
                .sum();
            let mut tasks = vec![
                schedule_total_unread_refresh(bridge),
                schedule_presence_channel_subscriptions(state, bridge),
                schedule_session_peer_presence_refresh(state, bridge),
            ];
            if state.session_list.refresh_pending {
                tasks.push(schedule_session_list_refresh(state, bridge));
            }
            Task::batch(tasks)
        }

        AppMessage::SessionListLoadFailed { error } => {
            state.session_list.is_loading = false;
            state.session_list.load_error = Some(format_ui_error(&error));
            if state.session_list.refresh_pending {
                schedule_session_list_refresh(state, bridge)
            } else {
                Task::none()
            }
        }

        AppMessage::TotalUnreadCountLoaded { count } => {
            // Keep badge consistent with current local session list projection, like privchat-app.
            if state.session_list.items.is_empty() {
                state.session_list.total_unread_count = count;
            } else {
                state.session_list.total_unread_count = state
                    .session_list
                    .items
                    .iter()
                    .map(|item| item.unread_count)
                    .sum();
            }
            Task::none()
        }

        AppMessage::TotalUnreadCountLoadFailed { error } => {
            state.session_list.load_error =
                Some(format!("UNREAD_COUNT_ERR: {}", format_ui_error(&error)));
            Task::none()
        }

        AppMessage::RefreshSessionList => {
            let mut tasks = vec![schedule_session_list_refresh(state, bridge)];
            if let Some(task) = schedule_active_conversation_refresh(state, bridge) {
                tasks.push(task);
            }
            Task::batch(tasks)
        }

        AppMessage::RefreshPresenceSnapshot => {
            if state.auth.user_id.is_none() {
                return Task::none();
            }
            match state.route {
                Route::AddFriend => schedule_friend_presence_refresh(state, bridge),
                Route::Chat | Route::SessionList => {
                    schedule_session_peer_presence_refresh(state, bridge)
                }
                _ => Task::none(),
            }
        }

        AppMessage::RepairChannelSyncRequested {
            channel_id,
            channel_type,
        } => schedule_channel_sync_repair(bridge, channel_id, channel_type),

        AppMessage::RepairChannelSyncSucceeded {
            channel_id,
            channel_type,
            applied,
        } => {
            if applied == 0 {
                return schedule_session_list_refresh(state, bridge);
            }

            let mut tasks = vec![
                schedule_session_list_refresh(state, bridge),
                schedule_total_unread_refresh(bridge),
            ];
            if state
                .active_chat
                .as_ref()
                .map(|chat| chat.channel_id == channel_id && chat.channel_type == channel_type)
                .unwrap_or(false)
            {
                tasks.push(handle_conversation_selected(
                    state,
                    bridge,
                    channel_id,
                    channel_type,
                ));
            }
            Task::batch(tasks)
        }

        AppMessage::RepairChannelSyncFailed {
            channel_id,
            channel_type,
            error,
        } => {
            warn!(
                "repair channel sync failed: channel_id={} channel_type={} error={}",
                channel_id,
                channel_type,
                format_ui_error(&error)
            );
            schedule_session_list_refresh(state, bridge)
        }

        AppMessage::RefreshAddFriendData => {
            if state.auth.user_id.is_none() {
                return Task::none();
            }
            state.add_friend.contacts_error = None;
            schedule_add_friend_refresh(bridge)
        }

        AppMessage::AddFriendFriendsLoaded { items } => {
            state.add_friend.friends = items;
            state.add_friend.contacts_error = None;
            apply_presence_to_friend_items(state);
            sync_add_friend_flags(state);
            schedule_friend_presence_refresh(state, bridge)
        }

        AppMessage::FriendPresencesLoaded { items } => {
            for presence in items {
                state.presences.insert(presence.user_id, presence);
            }
            apply_presence_to_friend_items(state);
            Task::none()
        }

        AppMessage::FriendPresencesLoadFailed { error } => {
            warn!(
                "friend presence refresh failed: {}",
                format_ui_error(&error)
            );
            Task::none()
        }

        AppMessage::AddFriendFriendsLoadFailed { error } => {
            state.add_friend.contacts_error = Some(format_ui_error(&error));
            Task::none()
        }

        AppMessage::AddFriendGroupsLoaded { items } => {
            state.add_friend.groups = items;
            state.add_friend.contacts_error = None;
            Task::none()
        }

        AppMessage::AddFriendGroupsLoadFailed { error } => {
            state.add_friend.contacts_error = Some(format_ui_error(&error));
            Task::none()
        }

        AppMessage::AddFriendRequestsLoaded { items } => {
            state.add_friend.requests = items;
            state.add_friend.contacts_error = None;
            sync_add_friend_flags(state);
            Task::none()
        }

        AppMessage::AddFriendRequestsLoadFailed { error } => {
            state.add_friend.contacts_error = Some(format_ui_error(&error));
            Task::none()
        }

        AppMessage::RefreshTotalUnreadCount => schedule_total_unread_refresh(bridge),

        AppMessage::ConnectionTitleStateChanged { state: next_state } => {
            let was_connected = matches!(
                state.connection_title_state,
                crate::app::message::ConnectionTitleState::Connected
            );
            state.connection_title_state = next_state;
            // When connection drops (was Connected/Authenticated, now reconnecting),
            // mark all online presences as offline so we don't show stale "在线".
            // Use current time as last_seen_at so they show "刚刚在线".
            if was_connected
                && matches!(
                    next_state,
                    crate::app::message::ConnectionTitleState::Connecting
                )
            {
                let now = chrono::Utc::now().timestamp();
                for presence in state.presences.values_mut() {
                    if presence.is_online {
                        presence.is_online = false;
                        presence.last_seen_at = now;
                    }
                }
            }
            Task::none()
        }

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

        AppMessage::FocusNextWidget { window_id } => {
            if state.main_window_id != Some(window_id) {
                return Task::none();
            }
            if matches!(state.route, Route::Login) {
                iced::widget::operation::focus_next()
            } else {
                Task::none()
            }
        }

        AppMessage::FocusPreviousWidget { window_id } => {
            if state.main_window_id != Some(window_id) {
                return Task::none();
            }
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

        AppMessage::GlobalLeftMousePressed { window_id } => {
            if state.main_window_id != Some(window_id) {
                return Task::none();
            }
            if let Some(chat) = &mut state.active_chat {
                chat.attachment_menu = None;
            }
            if let Some(cursor_x) = state.layout.last_cursor_x {
                if is_cursor_near_session_splitter(state, cursor_x) {
                    state.layout.is_resizing_session_splitter = true;
                }
            }
            Task::none()
        }

        AppMessage::GlobalCursorMoved { window_id, x } => {
            if state.main_window_id != Some(window_id) {
                return Task::none();
            }
            state.layout.last_cursor_x = Some(x);

            if !state.layout.is_resizing_session_splitter {
                return Task::none();
            }

            let target = x - SIDEBAR_WIDTH - PANEL_DIVIDER_WIDTH - (SESSION_SPLITTER_WIDTH * 0.5);
            state.layout.session_list_width =
                clamp_session_list_width(state.layout.window_width, target);
            Task::none()
        }

        AppMessage::WindowResized { window_id, width } => {
            if state.main_window_id != Some(window_id) {
                return Task::none();
            }
            state.layout.window_width = width;
            state.layout.session_list_width =
                clamp_session_list_width(width, state.layout.session_list_width);
            Task::none()
        }

        AppMessage::OpenSessionListPage => {
            state.overlay.settings_menu_open = false;
            // 用户回到会话列表时，明确退出“会话阅读态”，避免继续自动已读。
            leave_reading_conversation(state);
            state.route = Route::SessionList;
            Task::none()
        }

        AppMessage::MainWindowOpened { window_id } => {
            state.main_window_id = Some(window_id);
            Task::none()
        }

        AppMessage::ActivateMainWindow => {
            if let Some(id) = state.main_window_id {
                window::gain_focus(id)
            } else {
                Task::none()
            }
        }

        AppMessage::AddFriendSearchWindowOpened { window_id } => {
            state.add_friend_search_window_id = Some(window_id);
            Task::none()
        }

        AppMessage::OpenAddFriendPage => {
            state.overlay.settings_menu_open = false;
            leave_reading_conversation(state);
            state.route = Route::AddFriend;
            if state.auth.user_id.is_none() {
                return Task::none();
            }
            state.add_friend.feedback = None;
            state.add_friend.contacts_error = None;
            let mut tasks = vec![schedule_add_friend_refresh(bridge)];
            if !state.add_friend.friends.is_empty() {
                tasks.push(schedule_friend_presence_refresh(state, bridge));
            }
            Task::batch(tasks)
        }

        AppMessage::OpenAddFriendSearchWindow => {
            if let Some(window_id) = state.add_friend_search_window_id {
                return window::gain_focus(window_id);
            }

            let (window_id, task) = window::open(add_friend_search_window_settings());
            state.add_friend_search_window_id = Some(window_id);
            task.map(|window_id| AppMessage::AddFriendSearchWindowOpened { window_id })
        }

        AppMessage::CloseAddFriendSearchWindow => {
            if let Some(window_id) = state.add_friend_search_window_id.take() {
                return window::close(window_id);
            }
            Task::none()
        }

        AppMessage::WindowCloseRequested { window_id } => {
            let is_main = state.main_window_id == Some(window_id);

            // Clean up tracked sub-window state
            if state.add_friend_search_window_id == Some(window_id) {
                state.add_friend_search_window_id = None;
            }
            if state.logs_window_id == Some(window_id) {
                state.logs_window_id = None;
            }
            if state.image_viewer_window_id == Some(window_id) {
                state.image_viewer_window_id = None;
                state.image_viewer = None;
            }

            if is_main {
                state.main_window_id = None;
            }

            // If the main window is being closed (or already gone), exit the process.
            // daemon mode does not auto-exit when all windows close, so we
            // schedule a hard exit as a fallback.
            if is_main || state.main_window_id.is_none() {
                std::thread::spawn(|| {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    std::process::exit(0);
                });
                return iced::exit();
            }

            window::close(window_id)
        }

        AppMessage::ToggleSettingsMenu => {
            state.overlay.settings_menu_open = !state.overlay.settings_menu_open;
            Task::none()
        }

        AppMessage::DismissSettingsMenu => {
            state.overlay.settings_menu_open = false;
            Task::none()
        }

        AppMessage::SettingsMenuOpenSettings => {
            state.overlay.settings_menu_open = false;
            leave_reading_conversation(state);
            state.route = Route::Settings;
            Task::none()
        }

        AppMessage::SettingsMenuOpenLogs => {
            state.overlay.settings_menu_open = false;
            if let Some(window_id) = state.logs_window_id {
                return window::gain_focus(window_id);
            }
            let (window_id, task) = window::open(logs_window_settings());
            state.logs_window_id = Some(window_id);
            task.map(|window_id| AppMessage::LogsWindowOpened { window_id })
        }

        AppMessage::SettingsMenuSwitchAccount => {
            state.overlay.settings_menu_open = false;
            leave_reading_conversation(state);
            state.switch_account.loading = true;
            state.switch_account.switching_uid = None;
            state.switch_account.error = None;
            state.switch_account.return_route = state.route.clone();
            state.switch_account.add_account_login_mode = false;
            state.route = Route::SwitchAccount;
            schedule_local_accounts_refresh(bridge)
        }

        AppMessage::SettingsMenuLogout => {
            apply_logout(state);
            let bridge = Arc::clone(bridge);
            Task::perform(
                async move {
                    if let Err(error) = bridge.logout().await {
                        tracing::warn!("sdk logout failed: {:?}", error);
                    }
                },
                |_| AppMessage::Noop,
            )
        }

        AppMessage::LogsWindowOpened { window_id } => {
            state.logs_window_id = Some(window_id);
            Task::none()
        }

        AppMessage::CloseLogsWindow => {
            if let Some(window_id) = state.logs_window_id.take() {
                return window::close(window_id);
            }
            Task::none()
        }

        AppMessage::CopyLogsPressed => {
            let content = state
                .runtime_logs
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            match copy_text_to_clipboard(&content) {
                Ok(()) => {
                    state.settings.logs_feedback = Some("日志已复制到剪贴板".to_string());
                }
                Err(error) => {
                    let text = format!("复制日志失败: {}", format_ui_error(&error));
                    state.settings.logs_feedback = Some(text.clone());
                    append_runtime_log(state, "WARN", &text);
                }
            }
            Task::none()
        }

        AppMessage::ClearLogsPressed => {
            state.runtime_logs.clear();
            state.settings.logs_feedback = Some("日志已清空".to_string());
            Task::none()
        }

        AppMessage::ExportLogsPressed => Task::perform(
            async move {
                rfd::FileDialog::new()
                    .set_file_name("privchat-iced.log")
                    .save_file()
                    .map(|path| path.to_string_lossy().to_string())
            },
            |save_path| AppMessage::LogsExportSelected { save_path },
        ),

        AppMessage::LogsExportSelected { save_path } => {
            let Some(path) = save_path else {
                state.settings.logs_feedback = Some("已取消导出".to_string());
                return Task::none();
            };
            let content = state
                .runtime_logs
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            match fs::write(&path, content) {
                Ok(()) => {
                    state.settings.logs_feedback = Some(format!("日志已导出: {path}"));
                }
                Err(error) => {
                    let text = format!("导出日志失败: {error}");
                    state.settings.logs_feedback = Some(text.clone());
                    append_runtime_log(state, "WARN", &text);
                }
            }
            Task::none()
        }

        AppMessage::ToggleNotificationSound => {
            state.settings.notification_sound_enabled = !state.settings.notification_sound_enabled;
            state.settings.logs_feedback = Some(if state.settings.notification_sound_enabled {
                "已开启新消息提示音".to_string()
            } else {
                "已关闭新消息提示音".to_string()
            });
            Task::none()
        }

        AppMessage::CloseSwitchAccountPanel => {
            state.switch_account.loading = false;
            state.switch_account.switching_uid = None;
            state.switch_account.error = None;
            state.switch_account.add_account_login_mode = false;
            if state.auth.user_id.is_none() {
                state.route = Route::Login;
            } else {
                state.route = resolve_switch_account_return_route(state);
            }
            Task::none()
        }

        AppMessage::SwitchAccountListLoaded { accounts } => {
            state.switch_account.accounts = accounts;
            state.switch_account.loading = false;
            state.switch_account.error = None;
            Task::none()
        }

        AppMessage::SwitchAccountListLoadFailed { error } => {
            state.switch_account.loading = false;
            state.switch_account.error = Some(format_ui_error(&error));
            Task::none()
        }

        AppMessage::SwitchAccountPressed { uid } => {
            if uid.trim().is_empty() || state.switch_account.loading {
                return Task::none();
            }
            if state
                .switch_account
                .accounts
                .iter()
                .any(|account| account.uid == uid && account.is_active)
            {
                state.switch_account.loading = false;
                state.switch_account.switching_uid = None;
                state.switch_account.error = None;
                state.route = resolve_switch_account_return_route(state);
                return Task::none();
            }

            state.switch_account.loading = true;
            state.switch_account.switching_uid = Some(uid.clone());
            state.switch_account.error = None;
            // Force subscription teardown immediately when switch starts.
            // This prevents stale events from the old account leaking into UI
            // while switch_to_local_account is in-flight.
            state.session_epoch = state.session_epoch.wrapping_add(1);

            // Eagerly clear the old user's state so that any stale SDK events
            // (emitted during the in-flight switch_to_local_account task) land
            // on empty state and produce no harmful mutations.
            state.active_chat = None;
            leave_reading_conversation(state);
            state.session_list.items.clear();
            state.session_list.total_unread_count = 0;
            state.session_list.is_loading = false;
            state.session_list.refresh_pending = false;

            let bridge = Arc::clone(bridge);
            let uid_for_task = uid.clone();
            Task::perform(
                async move { bridge.switch_to_local_account(uid_for_task).await },
                move |result| match result {
                    Ok(session) => AppMessage::SwitchAccountSucceeded { uid, session },
                    Err(error) => AppMessage::SwitchAccountFailed { uid, error },
                },
            )
        }

        AppMessage::SwitchAccountAddPressed => {
            state.switch_account.add_account_login_mode = true;
            state.auth.is_submitting = false;
            state.auth.error = None;
            state.auth.password.clear();
            if Uuid::parse_str(state.auth.device_id.trim()).is_err() {
                state.auth.device_id = Uuid::new_v4().to_string();
            }
            state.route = Route::Login;
            Task::none()
        }

        AppMessage::SwitchAccountSucceeded { uid: _, session } => {
            apply_logout(state);
            // Keep the username as the login identifier the user entered.
            // Do not force it to local numeric uid after account switch.
            apply_login_success(state, session.user_id, session.token, session.device_id);
            state.switch_account.loading = false;
            state.switch_account.switching_uid = None;
            state.switch_account.error = None;
            state.switch_account.add_account_login_mode = false;
            // At this point session_epoch is bumped (via apply_login_success), so
            // the next subscription() call will create a fresh SDK event stream.
            // All data from sync_all_channels (run inside switch_to_local_account)
            // is already in the DB — just reload from it.
            Task::batch([
                schedule_session_list_refresh(state, bridge),
                schedule_total_unread_refresh(bridge),
                schedule_local_accounts_refresh(bridge),
                schedule_active_username_refresh(bridge),
            ])
        }

        AppMessage::ActiveUsernameLoaded { username } => {
            if !username.trim().is_empty() {
                state.auth.username = username;
            }
            Task::none()
        }

        AppMessage::ActiveUsernameLoadFailed { error } => {
            warn!("load_active_username failed: {}", format_ui_error(&error));
            Task::none()
        }

        AppMessage::SwitchAccountFailed { uid, error } => {
            state.switch_account.loading = false;
            state.switch_account.switching_uid = None;
            state.switch_account.error = Some(format!(
                "切换账号失败（{}）: {}",
                uid,
                format_ui_error(&error)
            ));
            // Ensure event stream can be rebuilt after a failed switch that may
            // have disconnected transport and closed receiver.
            state.session_epoch = state.session_epoch.wrapping_add(1);
            Task::none()
        }

        AppMessage::LoginBackPressed => {
            if state.switch_account.add_account_login_mode && state.auth.user_id.is_some() {
                state.switch_account.add_account_login_mode = false;
                state.auth.is_submitting = false;
                state.auth.error = None;
                state.auth.password.clear();
                state.route = Route::SwitchAccount;
                return schedule_local_accounts_refresh(bridge);
            }
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
                schedule_session_list_refresh(state, bridge),
                schedule_total_unread_refresh(bridge),
                schedule_local_accounts_refresh(bridge),
                schedule_active_username_refresh(bridge),
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
        } => {
            state.session_list.context_menu = None;
            handle_conversation_selected(state, bridge, channel_id, channel_type)
        }

        AppMessage::SessionListCursorMoved(point) => {
            state.session_list.last_cursor_pos = Some(point);
            Task::none()
        }

        AppMessage::SessionListItemRightClicked {
            channel_id,
            channel_type,
            is_pinned,
        } => {
            state.session_list.context_menu = Some(crate::app::state::SessionContextMenuState {
                channel_id,
                channel_type,
                is_pinned,
                anchor_pos: state.session_list.last_cursor_pos,
            });
            Task::none()
        }

        AppMessage::DismissSessionContextMenu => {
            state.session_list.context_menu = None;
            Task::none()
        }

        AppMessage::PinChannelPressed {
            channel_id,
            channel_type,
            pinned,
        } => {
            state.session_list.context_menu = None;
            let pin_bridge = Arc::clone(bridge);
            Task::perform(
                async move { pin_bridge.pin_channel(channel_id, pinned).await },
                move |result| AppMessage::PinChannelResolved {
                    channel_id,
                    channel_type,
                    result,
                },
            )
        }

        AppMessage::PinChannelResolved {
            channel_id,
            channel_type,
            result,
        } => match result {
            Ok(()) => {
                tracing::info!(
                    "pin_channel ok: channel_id={} channel_type={}",
                    channel_id,
                    channel_type
                );
                schedule_session_list_refresh(state, bridge)
            }
            Err(error) => {
                state.session_list.load_error =
                    Some(format!("PIN_ERR: {}", format_ui_error(&error)));
                Task::none()
            }
        },

        AppMessage::HideChannelPressed {
            channel_id,
            channel_type,
        } => {
            state.session_list.context_menu = None;
            let hide_bridge = Arc::clone(bridge);
            Task::perform(
                async move { hide_bridge.hide_channel(channel_id).await },
                move |result| AppMessage::HideChannelResolved {
                    channel_id,
                    channel_type,
                    result,
                },
            )
        }

        AppMessage::HideChannelResolved {
            channel_id,
            channel_type,
            result,
        } => match result {
            Ok(()) => {
                // Close chat if the hidden channel is currently active.
                if state
                    .active_chat
                    .as_ref()
                    .map(|chat| chat.channel_id == channel_id && chat.channel_type == channel_type)
                    .unwrap_or(false)
                {
                    leave_reading_conversation(state);
                    state.active_chat = None;
                    state.route = Route::SessionList;
                }
                schedule_session_list_refresh(state, bridge)
            }
            Err(error) => {
                state.session_list.load_error =
                    Some(format!("HIDE_ERR: {}", format_ui_error(&error)));
                Task::none()
            }
        },

        AppMessage::DeleteChannelPressed {
            channel_id,
            channel_type,
        } => {
            state.session_list.context_menu = None;
            let delete_bridge = Arc::clone(bridge);
            Task::perform(
                async move { delete_bridge.delete_channel_local(channel_id).await },
                move |result| AppMessage::DeleteChannelResolved {
                    channel_id,
                    channel_type,
                    result,
                },
            )
        }

        AppMessage::DeleteChannelResolved {
            channel_id,
            channel_type,
            result,
        } => match result {
            Ok(()) => {
                if state
                    .active_chat
                    .as_ref()
                    .map(|chat| chat.channel_id == channel_id && chat.channel_type == channel_type)
                    .unwrap_or(false)
                {
                    leave_reading_conversation(state);
                    state.active_chat = None;
                    state.route = Route::SessionList;
                }
                Task::batch([
                    schedule_session_list_refresh(state, bridge),
                    schedule_total_unread_refresh(bridge),
                ])
            }
            Err(error) => {
                state.session_list.load_error =
                    Some(format!("DELETE_CHANNEL_ERR: {}", format_ui_error(&error)));
                Task::none()
            }
        },

        AppMessage::ConversationOpened {
            channel_id,
            channel_type,
            open_token,
            snapshot,
            peer_read_pts,
        } => {
            if !pass_dual_guard(state, channel_id, channel_type, open_token) {
                return Task::none();
            }

            // Read Gate v1: Zombie Clear Defense.
            // If the user navigated away (e.g., back to list) before this async task completed,
            // active_read_channel_id will no longer match. We must NOT clear unread or activate context.
            if state.active_read_channel_id != Some(channel_id) {
                tracing::warn!(
                    "read_gate.zombie_clear_blocked: channel_id={} active_read_channel_id={:?}",
                    channel_id,
                    state.active_read_channel_id
                );
                return Task::none();
            }

            if let Some(chat) = &mut state.active_chat {
                chat.timeline.revision = snapshot.revision;
                chat.timeline.items = snapshot.items;
                normalize_timeline_items(&mut chat.timeline.items);
                chat.timeline.oldest_server_message_id = snapshot.oldest_server_message_id;
                chat.timeline.has_more_before = snapshot.has_more_before;
                chat.unread_marker = snapshot.unread_marker;
                chat.runtime_index.rebuild_from_items(&chat.timeline.items);
                if chat.peer_user_id.is_none() {
                    chat.peer_user_id =
                        infer_peer_user_id_from_timeline(&chat.timeline.items, state.auth.user_id);
                    tracing::info!(
                        "presence.infer_peer_from_timeline: channel_id={} channel_type={} peer_user_id={:?}",
                        chat.channel_id,
                        chat.channel_type,
                        chat.peer_user_id
                    );
                }
                if let Some(pts) = peer_read_pts {
                    chat.peer_last_read_pts = Some(pts);
                    tracing::info!(
                        "cold_start.peer_read_pts: channel_id={} peer_read_pts={}",
                        channel_id, pts
                    );
                }
            }
            let media_items = state
                .active_chat
                .as_ref()
                .map(|chat| chat.timeline.items.clone())
                .unwrap_or_default();
            let media_tasks = schedule_thumbnail_downloads_for_items(state, &media_items, bridge);
            let decode_tasks = schedule_image_decodes(state);
            let mut tasks = media_tasks;
            tasks.extend(decode_tasks);

            // Read Gate v1: Entering a conversation activates the read context
            enter_reading_conversation(state, channel_id);

            clear_local_unread_for_channel(state, channel_id, channel_type);
            let last_read_pts = state
                .active_chat
                .as_ref()
                .and_then(|chat| latest_read_pts(&chat.timeline.items))
                .unwrap_or(0);
            tasks.push(maybe_auto_mark_read(
                state,
                bridge,
                channel_id,
                channel_type,
                last_read_pts,
            ));
            // subscribe_channel requires network; run after timeline is loaded from local DB
            let subscribe_bridge = Arc::clone(bridge);
            tasks.push(Task::perform(
                async move {
                    subscribe_bridge
                        .subscribe_channel(channel_id, channel_type)
                        .await
                },
                move |result| {
                    match result {
                        Ok(()) => tracing::info!(
                            "presence.subscribe_channel.ok: channel_id={} channel_type={} open_token={}",
                            channel_id, channel_type, open_token
                        ),
                        Err(error) => tracing::warn!(
                            "presence.subscribe_channel.failed: channel_id={} channel_type={} open_token={} error={}",
                            channel_id, channel_type, open_token,
                            format_ui_error(&error)
                        ),
                    }
                    AppMessage::Noop
                },
            ));
            if let Some(chat) = state.active_chat.as_ref() {
                if let Some(peer_user_id) = chat.peer_user_id {
                    let presence_bridge = Arc::clone(bridge);
                    tasks.push(Task::perform(
                        async move { presence_bridge.batch_get_presence(vec![peer_user_id]).await },
                        move |result| match result {
                            Ok(mut items) => AppMessage::ChatPresenceLoaded {
                                channel_id,
                                channel_type,
                                open_token,
                                presence: items.pop(),
                            },
                            Err(error) => AppMessage::ChatPresenceLoadFailed {
                                channel_id,
                                channel_type,
                                open_token,
                                error,
                            },
                        },
                    ));
                }
            }
            tasks.push(Task::done(AppMessage::RequestReactionsRefresh {
                channel_id,
                channel_type,
                open_token,
            }));
            Task::batch(tasks)
        }

        AppMessage::ChatPresenceLoaded {
            channel_id,
            channel_type,
            open_token,
            presence,
        } => {
            if !pass_dual_guard(state, channel_id, channel_type, open_token) {
                return Task::none();
            }
            if let Some(presence) = presence {
                tracing::info!(
                    "presence.chat_loaded: channel_id={} channel_type={} open_token={} user_id={} is_online={} last_seen_at={} device_count={}",
                    channel_id,
                    channel_type,
                    open_token,
                    presence.user_id,
                    presence.is_online,
                    presence.last_seen_at,
                    presence.device_count
                );
                state.presences.insert(presence.user_id, presence);
                apply_presence_to_friend_items(state);
            } else {
                tracing::warn!(
                    "presence.chat_loaded: channel_id={} channel_type={} open_token={} empty result",
                    channel_id,
                    channel_type,
                    open_token
                );
            }
            Task::none()
        }

        AppMessage::ChatPresenceLoadFailed {
            channel_id,
            channel_type,
            open_token,
            error,
        } => {
            if !pass_dual_guard(state, channel_id, channel_type, open_token) {
                return Task::none();
            }
            tracing::warn!(
                "presence.chat_load_failed: channel_id={} channel_type={} open_token={} error={}",
                channel_id,
                channel_type,
                open_token,
                format_ui_error(&error)
            );
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

        AppMessage::ActiveConversationRefreshed {
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
                chat.timeline.oldest_server_message_id = snapshot.oldest_server_message_id;
                chat.timeline.has_more_before = snapshot.has_more_before;
                chat.unread_marker = snapshot.unread_marker;

                // 核心修复：在覆盖时间线前，抢救那些还在发送中的本地消息
                // 1. 提取 pending locals (必须是 is_own 且正在发送中)
                // 严格基于 send_state 判断，避免误判其他无 server_message_id 的项
                let pending_locals: Vec<MessageVm> = chat
                    .timeline
                    .items
                    .iter()
                    .filter(|item| {
                        item.is_own
                            && item.client_txn_id.is_some()
                            && matches!(
                                item.send_state,
                                Some(MessageSendStateVm::Queued)
                                    | Some(MessageSendStateVm::Sending)
                                    | Some(MessageSendStateVm::Retrying)
                            )
                    })
                    .cloned()
                    .collect();

                // 2. 应用数据库快照 (这是持久态真相)
                let mut merged_items = snapshot.items;

                // 3. 将 pending locals 合并回快照中 (瞬时态乐观更新)
                // 只有当快照中不存在该消息时才追加 (防止 ack 后重复)
                // 去重锚点：client_txn_id (假设全链路透传)
                for local in pending_locals {
                    if let Some(local_txn_id) = local.client_txn_id {
                        let exists_in_snapshot = merged_items
                            .iter()
                            .any(|item| item.client_txn_id == Some(local_txn_id));
                        if !exists_in_snapshot {
                            merged_items.push(local);
                        }
                    }
                }

                // 4. 统一排序和重建索引
                // 必须调用 normalize 以确保顺序正确 (取决于 message_id 排序规则)
                // 同时也防止 normalize 中有其他清理逻辑
                normalize_timeline_items(&mut merged_items);
                chat.timeline.items = merged_items;
                chat.runtime_index.rebuild_from_items(&chat.timeline.items);
            }
            let media_items = state
                .active_chat
                .as_ref()
                .map(|chat| chat.timeline.items.clone())
                .unwrap_or_default();
            let mut tasks = schedule_thumbnail_downloads_for_items(state, &media_items, bridge);
            tasks.push(schedule_total_unread_refresh(bridge));
            Task::batch(tasks)
        }

        AppMessage::ActiveConversationRefreshFailed {
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

        AppMessage::PresenceChanged { presence } => {
            tracing::info!(
                "presence.event_changed: user_id={} is_online={} last_seen_at={} device_count={}",
                presence.user_id,
                presence.is_online,
                presence.last_seen_at,
                presence.device_count
            );
            state.presences.insert(presence.user_id, presence.clone());
            apply_presence_to_friend_items(state);
            Task::none()
        }

        AppMessage::PeerReadPtsAdvanced {
            channel_id,
            channel_type: _,
            reader_id: _,
            read_pts,
        } => {
            if let Some(chat) = &mut state.active_chat {
                if chat.channel_id == channel_id {
                    let old_pts = chat.peer_last_read_pts;
                    let new_pts = old_pts.unwrap_or(0).max(read_pts);
                    chat.peer_last_read_pts = Some(new_pts);
                    tracing::info!(
                        "peer_read_pts_advanced: channel_id={} event_pts={} old={:?} -> new={}",
                        channel_id, read_pts, old_pts, new_pts
                    );
                }
            }
            Task::none()
        }

        AppMessage::MessageDelivered {
            channel_id,
            channel_type: _,
            server_message_id,
        } => {
            if let Some(chat) = &mut state.active_chat {
                if chat.channel_id == channel_id {
                    for msg in &mut chat.timeline.items {
                        if msg.server_message_id == Some(server_message_id) {
                            msg.delivered = true;
                            tracing::info!(
                                "message_delivered: channel_id={} server_message_id={}",
                                channel_id, server_message_id
                            );
                            break;
                        }
                    }
                }
            }
            Task::none()
        }

        AppMessage::TypingStatusChanged {
            channel_id,
            channel_type,
            user_id,
            is_typing,
        } => {
            let my_user_id = state.auth.user_id.unwrap_or_default();
            if user_id == my_user_id {
                return Task::none();
            }
            if let Some(chat) = &mut state.active_chat {
                if chat.channel_id == channel_id && chat.channel_type == channel_type {
                    chat.typing_hint = if is_typing {
                        Some("对方正在输入…".to_string())
                    } else {
                        None
                    };
                    // 记录正在输入的用户 ID
                    chat.typing_user_id = if is_typing { Some(user_id) } else { None };
                }
            }
            if is_typing {
                schedule_typing_hint_expire_task(channel_id, channel_type, user_id)
            } else {
                Task::none()
            }
        }

        AppMessage::TypingHintExpired {
            channel_id,
            channel_type,
            user_id: _,
        } => {
            if let Some(chat) = &mut state.active_chat {
                if chat.channel_id == channel_id && chat.channel_type == channel_type {
                    chat.typing_hint = None;
                    chat.typing_user_id = None;
                }
            }
            Task::none()
        }

        AppMessage::ClearTypingIfMatch {
            channel_id,
            channel_type,
            user_id,
        } => {
            if let Some(chat) = &mut state.active_chat {
                if chat.channel_id == channel_id
                    && chat.channel_type == channel_type
                    && chat.typing_user_id == Some(user_id)
                {
                    chat.typing_hint = None;
                    chat.typing_user_id = None;
                }
            }
            Task::none()
        }

        AppMessage::TypingSendCompleted { is_typing } => {
            tracing::debug!("typing send completed: is_typing={is_typing}");
            Task::none()
        }

        AppMessage::TypingSendFailed { is_typing, error } => {
            warn!(
                "typing send failed: is_typing={} error={}",
                is_typing,
                format_ui_error(&error)
            );
            Task::none()
        }

        AppMessage::AddFriendInputChanged { text } => {
            state.add_friend.add_input = text;
            state.add_friend.feedback = None;
            state.add_friend.search_error = None;
            Task::none()
        }

        AppMessage::AddFriendSearchChanged { text } => {
            state.add_friend.search_input = text;
            Task::none()
        }

        AppMessage::AddFriendSearchPressed => {
            let query = state.add_friend.add_input.trim().to_string();
            if query.is_empty() {
                state.add_friend.search_error = Some("请输入用户名或 UID".to_string());
                state.add_friend.search_loading = false;
                state.add_friend.search_results.clear();
                state.add_friend.selected_search_user_id = None;
                return Task::none();
            }

            state.add_friend.search_loading = true;
            state.add_friend.search_error = None;
            state.add_friend.feedback = None;
            state.add_friend.search_results.clear();
            state.add_friend.selected_search_user_id = None;

            let bridge = Arc::clone(bridge);
            Task::perform(
                async move { bridge.search_users(query).await },
                |result| match result {
                    Ok(users) => AppMessage::AddFriendSearchLoaded { users },
                    Err(error) => AppMessage::AddFriendSearchFailed { error },
                },
            )
        }

        AppMessage::AddFriendSearchLoaded { users } => {
            let friend_ids = state
                .add_friend
                .friends
                .iter()
                .map(|item| item.user_id)
                .collect::<HashSet<_>>();
            let mut users = users;
            for user in &mut users {
                if friend_ids.contains(&user.user_id) {
                    user.is_friend = true;
                }
            }
            state.add_friend.search_loading = false;
            state.add_friend.search_results = users;
            state.add_friend.selected_search_user_id = state
                .add_friend
                .search_results
                .first()
                .map(|user| user.user_id);
            if state.add_friend.search_results.is_empty() {
                state.add_friend.search_error = Some("未找到匹配用户".to_string());
            } else {
                state.add_friend.search_error = None;
            }
            Task::none()
        }

        AppMessage::AddFriendSearchFailed { error } => {
            state.add_friend.search_loading = false;
            state.add_friend.search_results.clear();
            state.add_friend.selected_search_user_id = None;
            state.add_friend.search_error = Some(format_ui_error(&error));
            Task::none()
        }

        AppMessage::AddFriendResultSelected { user_id } => {
            state.add_friend.selected_search_user_id = Some(user_id);
            state.add_friend.feedback = None;
            Task::none()
        }

        AppMessage::AddFriendPanelSelected { item } => {
            state.add_friend.selected_panel_item = Some(item);
            state.add_friend.detail_loading = true;
            state.add_friend.detail = None;
            state.add_friend.detail_error = None;
            schedule_add_friend_detail_load(bridge, item)
        }

        AppMessage::AddFriendDetailLoaded { item, detail } => {
            if state.add_friend.selected_panel_item != Some(item) {
                return Task::none();
            }
            state.add_friend.detail_loading = false;
            state.add_friend.detail_error = None;
            state.add_friend.detail = Some(detail);
            Task::none()
        }

        AppMessage::AddFriendDetailLoadFailed { item, error } => {
            if state.add_friend.selected_panel_item != Some(item) {
                return Task::none();
            }
            state.add_friend.detail_loading = false;
            state.add_friend.detail = None;
            state.add_friend.detail_error = Some(format_ui_error(&error));
            Task::none()
        }

        AppMessage::AddFriendDetailSendMessagePressed { user_id } => {
            state.add_friend.detail_error = None;
            let bridge = Arc::clone(bridge);
            Task::perform(
                async move { bridge.get_or_create_direct_channel(user_id).await },
                move |result| match result {
                    Ok((channel_id, channel_type)) => {
                        AppMessage::AddFriendOpenConversationResolved {
                            user_id,
                            channel_id,
                            channel_type,
                        }
                    }
                    Err(error) => AppMessage::AddFriendOpenConversationFailed { user_id, error },
                },
            )
        }

        AppMessage::AddFriendOpenConversationResolved {
            user_id,
            channel_id,
            channel_type,
        } => {
            state.add_friend.feedback = Some(format!("正在打开与 {user_id} 的会话..."));
            Task::batch([
                handle_conversation_selected(state, bridge, channel_id, channel_type),
                schedule_session_list_refresh(state, bridge),
            ])
        }

        AppMessage::AddFriendOpenConversationFailed { user_id, error } => {
            state.add_friend.detail_error = Some(format!(
                "打开与 {user_id} 的会话失败: {}",
                format_ui_error(&error)
            ));
            Task::none()
        }

        AppMessage::AddFriendDetailAcceptRequestPressed { user_id } => {
            let already_friend = state
                .add_friend
                .friends
                .iter()
                .any(|friend| friend.user_id == user_id);
            if already_friend {
                state.add_friend.feedback = Some("该用户已是好友，正在打开会话...".to_string());
                return Task::batch([
                    schedule_add_friend_refresh(bridge),
                    Task::perform(
                        {
                            let bridge = Arc::clone(bridge);
                            async move { bridge.get_or_create_direct_channel(user_id).await }
                        },
                        move |result| match result {
                            Ok((channel_id, channel_type)) => {
                                AppMessage::AddFriendOpenConversationResolved {
                                    user_id,
                                    channel_id,
                                    channel_type,
                                }
                            }
                            Err(error) => {
                                AppMessage::AddFriendOpenConversationFailed { user_id, error }
                            }
                        },
                    ),
                ]);
            }

            state.add_friend.feedback = Some("同意好友申请中...".to_string());

            let bridge = Arc::clone(bridge);
            Task::perform(
                async move { bridge.accept_friend_request(user_id).await },
                move |result| match result {
                    Ok(user_id) => AppMessage::AddFriendAcceptSucceeded { user_id },
                    Err(error) => AppMessage::AddFriendAcceptFailed { user_id, error },
                },
            )
        }

        AppMessage::AddFriendAcceptSucceeded { user_id } => {
            state.add_friend.feedback = Some("已同意好友申请，正在打开会话...".to_string());
            Task::batch([
                schedule_add_friend_refresh(bridge),
                Task::perform(
                    {
                        let bridge = Arc::clone(bridge);
                        async move { bridge.get_or_create_direct_channel(user_id).await }
                    },
                    move |result| match result {
                        Ok((channel_id, channel_type)) => {
                            AppMessage::AddFriendOpenConversationResolved {
                                user_id,
                                channel_id,
                                channel_type,
                            }
                        }
                        Err(error) => {
                            AppMessage::AddFriendOpenConversationFailed { user_id, error }
                        }
                    },
                ),
            ])
        }

        AppMessage::AddFriendAcceptFailed { user_id, error } => {
            state.add_friend.detail_error = Some(format!(
                "同意 {user_id} 的好友申请失败: {}",
                format_ui_error(&error)
            ));
            Task::none()
        }

        AppMessage::ToggleNewFriendsSection => {
            state.add_friend.new_friends_expanded = !state.add_friend.new_friends_expanded;
            Task::none()
        }

        AppMessage::ToggleGroupSection => {
            state.add_friend.groups_expanded = !state.add_friend.groups_expanded;
            Task::none()
        }

        AppMessage::ToggleFriendSection => {
            state.add_friend.friends_expanded = !state.add_friend.friends_expanded;
            Task::none()
        }

        AppMessage::AddFriendRequestPressed => {
            let selected = state
                .add_friend
                .selected_search_user_id
                .and_then(|selected_user_id| {
                    state
                        .add_friend
                        .search_results
                        .iter()
                        .find(|user| user.user_id == selected_user_id)
                        .cloned()
                })
                .or_else(|| state.add_friend.search_results.first().cloned());

            let Some(target) = selected else {
                state.add_friend.feedback = Some("请先搜索并选择用户".to_string());
                return Task::none();
            };

            if target.is_friend {
                state.add_friend.feedback = Some("该用户已是好友".to_string());
                return Task::none();
            }

            state.add_friend.feedback = Some("发送好友申请中...".to_string());

            let bridge = Arc::clone(bridge);
            Task::perform(
                async move {
                    bridge
                        .send_friend_request(target.user_id, None, Some(target.search_session_id))
                        .await
                },
                |result| match result {
                    Ok(user_id) => AppMessage::AddFriendRequestSucceeded { user_id },
                    Err(error) => AppMessage::AddFriendRequestFailed { error },
                },
            )
        }

        AppMessage::AddFriendRequestSucceeded { user_id } => {
            for user in &mut state.add_friend.search_results {
                if user.user_id == user_id {
                    user.is_friend = true;
                }
            }
            state.add_friend.feedback = Some("好友申请已发送".to_string());
            schedule_add_friend_refresh(bridge)
        }

        AppMessage::AddFriendRequestFailed { error } => {
            state.add_friend.feedback = Some(format!("发送失败: {}", format_ui_error(&error)));
            Task::none()
        }

        AppMessage::CopyDetailFieldPressed { label, value } => {
            match copy_text_to_clipboard(&value) {
                Ok(()) => {
                    state.add_friend.feedback = Some(format!("已复制{label}"));
                }
                Err(error) => {
                    state.add_friend.feedback =
                        Some(format!("复制失败: {}", format_ui_error(&error)));
                }
            }
            Task::none()
        }

        AppMessage::ComposerPastePressed => handle_composer_paste_pressed(state, bridge),

        AppMessage::ComposerInputChanged { text } => {
            if let Some(chat) = &mut state.active_chat {
                chat.composer.draft = text;
                chat.composer.editor =
                    iced::widget::text_editor::Content::with_text(&chat.composer.draft);
                let is_typing = !chat.composer.draft.trim().is_empty();
                chat.composer.typing_active = is_typing;
                // 输入内容变化时立即上报（有输入 → typing=true，清空 → typing=false）
                // 服务端有 500ms 限流器控制频率，客户端不做 edge 判断
                return schedule_send_typing_task(
                    bridge,
                    chat.channel_id,
                    chat.channel_type,
                    is_typing,
                );
            }
            Task::none()
        }

        AppMessage::ToggleEmojiPicker => {
            if let Some(chat) = &mut state.active_chat {
                chat.composer.emoji_picker_open = !chat.composer.emoji_picker_open;
                chat.composer.quick_phrase_open = false;
            }
            Task::none()
        }

        AppMessage::DismissEmojiPicker => {
            if let Some(chat) = &mut state.active_chat {
                chat.composer.emoji_picker_open = false;
            }
            Task::none()
        }

        AppMessage::ToggleQuickPhrase => {
            if let Some(chat) = &mut state.active_chat {
                chat.composer.quick_phrase_open = !chat.composer.quick_phrase_open;
                chat.composer.emoji_picker_open = false;
                if chat.composer.quick_phrase_open {
                    // Load phrases from local store
                    let bridge = bridge.clone();
                    return Task::perform(
                        async move { bridge.load_quick_phrases().await.unwrap_or_default() },
                        |phrases| AppMessage::QuickPhrasesLoaded { phrases },
                    );
                }
            }
            Task::none()
        }

        AppMessage::DismissQuickPhrase => {
            if let Some(chat) = &mut state.active_chat {
                chat.composer.quick_phrase_open = false;
            }
            Task::none()
        }

        AppMessage::QuickPhrasesLoaded { phrases } => {
            if let Some(chat) = &mut state.active_chat {
                chat.composer.quick_phrases = phrases;
            }
            Task::none()
        }

        AppMessage::QuickPhrasePicked { index } => {
            if let Some(chat) = &mut state.active_chat {
                if let Some(phrase) = chat.composer.quick_phrases.get(index).cloned() {
                    chat.composer.draft = phrase;
                    chat.composer.editor =
                        iced::widget::text_editor::Content::with_text(&chat.composer.draft);
                    chat.composer.editor.perform(
                        iced::widget::text_editor::Action::Move(
                            iced::widget::text_editor::Motion::DocumentEnd,
                        ),
                    );
                    chat.composer.quick_phrase_open = false;
                    return iced::widget::operation::focus("chat-composer-editor");
                }
            }
            Task::none()
        }

        AppMessage::QuickPhraseDelete { index } => {
            if let Some(chat) = &mut state.active_chat {
                if index < chat.composer.quick_phrases.len() {
                    chat.composer.quick_phrases.remove(index);
                    let phrases = chat.composer.quick_phrases.clone();
                    let bridge = bridge.clone();
                    return Task::perform(
                        async move { bridge.save_quick_phrases(&phrases).await },
                        |_| AppMessage::Noop,
                    );
                }
            }
            Task::none()
        }

        AppMessage::OpenAddQuickPhrase => {
            if let Some(chat) = &mut state.active_chat {
                chat.composer.quick_phrase_adding = true;
                chat.composer.quick_phrase_input = String::new();
            }
            Task::none()
        }

        AppMessage::QuickPhraseInputChanged(value) => {
            if let Some(chat) = &mut state.active_chat {
                chat.composer.quick_phrase_input = value;
            }
            Task::none()
        }

        AppMessage::QuickPhraseConfirmAdd => {
            if let Some(chat) = &mut state.active_chat {
                let input = chat.composer.quick_phrase_input.trim().to_string();
                if !input.is_empty() {
                    chat.composer.quick_phrases.push(input);
                    let phrases = chat.composer.quick_phrases.clone();
                    let bridge = bridge.clone();
                    chat.composer.quick_phrase_adding = false;
                    chat.composer.quick_phrase_input.clear();
                    return Task::perform(
                        async move { bridge.save_quick_phrases(&phrases).await },
                        |_| AppMessage::Noop,
                    );
                }
                chat.composer.quick_phrase_adding = false;
                chat.composer.quick_phrase_input.clear();
            }
            Task::none()
        }

        AppMessage::QuickPhraseCancelAdd => {
            if let Some(chat) = &mut state.active_chat {
                chat.composer.quick_phrase_adding = false;
                chat.composer.quick_phrase_input.clear();
            }
            Task::none()
        }

        AppMessage::QuickPhraseAdded { phrase } => {
            if let Some(chat) = &mut state.active_chat {
                if !phrase.trim().is_empty() {
                    chat.composer.quick_phrases.push(phrase);
                    let phrases = chat.composer.quick_phrases.clone();
                    let bridge = bridge.clone();
                    return Task::perform(
                        async move { bridge.save_quick_phrases(&phrases).await },
                        |_| AppMessage::Noop,
                    );
                }
            }
            Task::none()
        }

        AppMessage::EmojiPicked { emoji } => {
            if let Some(chat) = &mut state.active_chat {
                let was_typing = chat.composer.typing_active;
                chat.composer.draft.push_str(&emoji);
                chat.composer.editor =
                    iced::widget::text_editor::Content::with_text(&chat.composer.draft);
                chat.composer.emoji_picker_open = false;
                chat.composer.quick_phrase_open = false;
                let is_typing = !chat.composer.draft.trim().is_empty();
                chat.composer.typing_active = is_typing;
                if was_typing != is_typing {
                    return schedule_send_typing_task(
                        bridge,
                        chat.channel_id,
                        chat.channel_type,
                        is_typing,
                    );
                }
            }
            Task::none()
        }

        AppMessage::ComposerPickImagePressed => Task::perform(
            async move {
                rfd::FileDialog::new()
                    .add_filter(
                        "Images",
                        &["png", "jpg", "jpeg", "gif", "webp", "bmp", "heic"],
                    )
                    .pick_file()
                    .map(|path| path.to_string_lossy().to_string())
            },
            |path| AppMessage::ComposerAttachmentPicked { path },
        ),

        AppMessage::ComposerPickFilePressed => Task::perform(
            async move {
                rfd::FileDialog::new()
                    .pick_file()
                    .map(|path| path.to_string_lossy().to_string())
            },
            |path| AppMessage::ComposerAttachmentPicked { path },
        ),

        AppMessage::ComposerAttachmentPicked { path } => {
            if let Some(chat) = &mut state.active_chat {
                chat.composer.pending_attachment = path.and_then(|value| {
                    let trimmed = value.trim().to_string();
                    if trimmed.is_empty() {
                        return None;
                    }
                    let filename = Path::new(&trimmed)
                        .file_name()
                        .and_then(|part| part.to_str())
                        .unwrap_or("file")
                        .to_string();
                    Some(PendingAttachmentState {
                        is_image: is_image_file_path(&trimmed),
                        path: trimmed,
                        filename,
                    })
                });
            }
            Task::none()
        }

        AppMessage::ComposerAttachmentSendConfirmed => {
            let Some(chat) = state.active_chat.as_ref() else {
                return Task::none();
            };
            let Some(path) = chat
                .composer
                .pending_attachment
                .as_ref()
                .map(|pending| pending.path.clone())
            else {
                return Task::none();
            };
            if let Some(chat) = &mut state.active_chat {
                chat.composer.pending_attachment = None;
            }
            handle_send_attachment_path(state, bridge, path)
        }

        AppMessage::ComposerAttachmentSendCanceled => {
            if let Some(chat) = &mut state.active_chat {
                chat.composer.pending_attachment = None;
            }
            Task::none()
        }

        AppMessage::OpenImagePreview {
            message_id,
            original_path,
            thumbnail_path,
            media_url,
            file_id,
            created_at,
        } => {
            // 如果已有图片查看器窗口，先关闭
            if let Some(wid) = state.image_viewer_window_id.take() {
                let _: Task<AppMessage> = window::close(wid);
            }

            let original_exists = original_path
                .as_ref()
                .map(|p| Path::new(p).exists())
                .unwrap_or(false);
            let thumbnail_exists = thumbnail_path
                .as_ref()
                .map(|p| Path::new(p).exists())
                .unwrap_or(false);

            // 显示路径必须指向存在的本地文件，否则 iced::widget::image() 加载失败时
            // 整个 viewer 区域会变成 base container 的深色背景（看起来"黑屏"）。
            // 留空字符串可让 viewer 走"加载中..."提示分支，体感更友好。
            let display_path = if original_exists {
                original_path.clone().unwrap()
            } else if thumbnail_exists {
                thumbnail_path.clone().unwrap()
            } else {
                String::new()
            };

            let will_download = !original_exists && (file_id.is_some() || media_url.is_some());
            let viewer_state = crate::app::state::ImageViewerState {
                message_id,
                image_path: display_path,
                loading_original: will_download,
                original_path: if original_exists { original_path } else { None },
                thumbnail_path: if thumbnail_exists { thumbnail_path.clone() } else { None },
                title: "图片查看器".to_string(),
                download_progress: None,
            };
            state.image_viewer = Some(viewer_state);

            let (window_id, open_task) = window::open(image_viewer_window_settings());
            state.image_viewer_window_id = Some(window_id);

            let open_task = open_task
                .map(|wid| AppMessage::ImageViewerWindowOpened { window_id: wid });

            if will_download {
                // Kick off a streaming SDK download; progress arrives via
                // `MediaDownloadStateChanged` and is consumed by the viewer.
                let bridge = bridge.clone();
                let created_at_ms = if created_at > 9_999_999_999 {
                    created_at
                } else {
                    created_at * 1000
                };

                let download_task = Task::perform(
                    async move {
                        let url = if let Some(fid) = file_id {
                            bridge.get_file_url(fid).await?
                        } else {
                            media_url.unwrap_or_default()
                        };
                        if url.is_empty() {
                            return Err(crate::presentation::vm::UiError::Unknown(
                                "image url missing".to_string(),
                            ));
                        }
                        bridge
                            .start_message_media_download(
                                message_id,
                                url,
                                "image/*".to_string(),
                                None,
                                created_at_ms,
                            )
                            .await?;
                        Ok::<(), crate::presentation::vm::UiError>(())
                    },
                    move |result| match result {
                        Ok(()) => AppMessage::Noop,
                        Err(error) => AppMessage::ImageOriginalFailed { message_id, error },
                    },
                );

                Task::batch([open_task, download_task])
            } else {
                open_task
            }
        }

        AppMessage::ImageViewerWindowOpened { window_id: _ } => {
            Task::none()
        }

        AppMessage::CloseImageViewerWindow => {
            if let Some(wid) = state.image_viewer_window_id.take() {
                state.image_viewer = None;
                return window::close(wid);
            }
            Task::none()
        }

        AppMessage::ImageOriginalReady {
            message_id,
            local_path,
        } => {
            if let Some(viewer) = &mut state.image_viewer {
                if viewer.message_id == message_id {
                    viewer.image_path = local_path.clone();
                    viewer.original_path = Some(local_path);
                    viewer.loading_original = false;
                    viewer.download_progress = None;
                }
            }
            Task::none()
        }

        AppMessage::ImageOriginalFailed {
            message_id,
            error,
        } => {
            tracing::warn!("下载原图失败 message_id={}: {:?}", message_id, error);
            if let Some(viewer) = &mut state.image_viewer {
                if viewer.message_id == message_id {
                    viewer.loading_original = false;
                    viewer.download_progress = None;
                }
            }
            Task::none()
        }

        AppMessage::MediaDownloadStateChanged { message_id, state: dl_state } => {
            use privchat_sdk::MediaDownloadState as Dl;
            // Update image viewer if this is the active preview.
            if let Some(viewer) = &mut state.image_viewer {
                if viewer.message_id == message_id {
                    match &dl_state {
                        Dl::Downloading { bytes, total } => {
                            viewer.loading_original = true;
                            viewer.download_progress = Some((*bytes, *total));
                        }
                        Dl::Paused { bytes, total } => {
                            viewer.loading_original = true;
                            viewer.download_progress = Some((*bytes, *total));
                        }
                        Dl::Done { path } => {
                            viewer.image_path = path.clone();
                            viewer.original_path = Some(path.clone());
                            viewer.loading_original = false;
                            viewer.download_progress = None;
                        }
                        Dl::Failed { code, message } => {
                            tracing::warn!(
                                "image viewer download failed message_id={} code={} message={}",
                                message_id, code, message
                            );
                            viewer.loading_original = false;
                            viewer.download_progress = None;
                        }
                        Dl::Idle => {
                            viewer.loading_original = false;
                            viewer.download_progress = None;
                        }
                    }
                }
            }
            Task::none()
        }

        AppMessage::OpenAttachment {
            message_id,
            created_at,
            local_path,
            file_id,
            filename,
        } => {
            if let Some(local) = local_path {
                let path = Path::new(&local);
                if path.exists() {
                    return match reveal_in_file_manager(&local) {
                        Ok(()) => Task::none(),
                        Err(error) => {
                            Task::done(AppMessage::AttachmentOpenResolved { result: Err(error) })
                        }
                    };
                }
            }
            let Some(file_id) = file_id else {
                return Task::done(AppMessage::AttachmentOpenResolved {
                    result: Err(crate::presentation::vm::UiError::Unknown(
                        "attachment file_id missing".to_string(),
                    )),
                });
            };
            let bridge = Arc::clone(bridge);

            Task::perform(
                async move {
                    let url = bridge.get_file_url(file_id).await?;
                    let path = bridge
                        .download_message_media_blocking(
                            message_id,
                            url,
                            String::new(),
                            filename,
                            created_at,
                        )
                        .await?;
                    reveal_in_file_manager(&path)?;
                    Ok(path)
                },
                |result| AppMessage::AttachmentOpenResolved { result },
            )
        }

        AppMessage::OpenVideo {
            message_id,
            created_at,
            local_path,
            file_id,
            filename,
        } => {
            if let Some(local) = local_path {
                let path = Path::new(&local);
                if path.exists() {
                    return match open_with_system(&local) {
                        Ok(()) => Task::none(),
                        Err(error) => {
                            Task::done(AppMessage::AttachmentOpenResolved { result: Err(error) })
                        }
                    };
                }
            }
            let Some(file_id) = file_id else {
                return Task::done(AppMessage::AttachmentOpenResolved {
                    result: Err(crate::presentation::vm::UiError::Unknown(
                        "video file_id missing".to_string(),
                    )),
                });
            };
            let bridge = Arc::clone(bridge);

            Task::perform(
                async move {
                    let url = bridge.get_file_url(file_id).await?;
                    // Derive filename_hint from the resolved video download URL (the
                    // bubble has no usable filename — `message.media_url` is the
                    // thumbnail). Strip query string / fragment and require a plain
                    // segment with an extension.
                    let hint = url
                        .rsplit('/')
                        .next()
                        .map(|s| s.split(['?', '#']).next().unwrap_or(s).to_string())
                        .filter(|s| !s.is_empty() && s.contains('.'))
                        .or(filename);
                    let path = bridge
                        .download_message_media_blocking(
                            message_id,
                            url,
                            String::new(),
                            hint,
                            created_at,
                        )
                        .await?;
                    open_with_system(&path)?;
                    Ok(path)
                },
                |result| AppMessage::AttachmentOpenResolved { result },
            )
        }

        AppMessage::ShowAttachmentMenu {
            message_id,
            created_at,
            local_path,
            file_id,
            filename,
        } => {
            if let Some(chat) = &mut state.active_chat {
                let snapshot = chat
                    .timeline
                    .items
                    .iter()
                    .find(|m| m.message_id == message_id)
                    .cloned();
                let (channel_id, channel_type, message_key, server_message_id, is_own, is_revoked, send_state, from_uid) =
                    if let Some(msg) = snapshot {
                        (
                            msg.channel_id,
                            msg.channel_type,
                            msg.key,
                            msg.server_message_id,
                            msg.is_own,
                            msg.is_deleted,
                            msg.send_state,
                            msg.from_uid,
                        )
                    } else {
                        (chat.channel_id, chat.channel_type, crate::presentation::vm::TimelineItemKey::default(), None, false, false, None, 0)
                    };
                chat.attachment_menu = Some(crate::app::state::AttachmentMenuState {
                    message_id,
                    channel_id,
                    channel_type,
                    open_token: chat.open_token,
                    message_key,
                    server_message_id,
                    is_own,
                    is_revoked,
                    is_attachment: true,
                    send_state,
                    created_at,
                    local_path,
                    file_id,
                    filename: filename.clone(),
                    copy_text: None,
                    anchor_pos: chat.last_cursor_pos,
                    from_uid,
                    reply_preview: filename,
                });
            }
            Task::none()
        }

        AppMessage::ShowTextMenu { message_id, text } => {
            if let Some(chat) = &mut state.active_chat {
                let snapshot = chat
                    .timeline
                    .items
                    .iter()
                    .find(|m| m.message_id == message_id)
                    .cloned();
                let (channel_id, channel_type, message_key, server_message_id, is_own, is_revoked, send_state, created_at, from_uid) =
                    if let Some(msg) = snapshot {
                        (
                            msg.channel_id,
                            msg.channel_type,
                            msg.key,
                            msg.server_message_id,
                            msg.is_own,
                            msg.is_deleted,
                            msg.send_state,
                            msg.created_at,
                            msg.from_uid,
                        )
                    } else {
                        (chat.channel_id, chat.channel_type, crate::presentation::vm::TimelineItemKey::default(), None, false, false, None, 0, 0)
                    };
                chat.attachment_menu = Some(crate::app::state::AttachmentMenuState {
                    message_id,
                    channel_id,
                    channel_type,
                    open_token: chat.open_token,
                    message_key,
                    server_message_id,
                    is_own,
                    is_revoked,
                    is_attachment: false,
                    send_state,
                    created_at,
                    local_path: None,
                    file_id: None,
                    filename: String::new(),
                    copy_text: Some(text.clone()),
                    anchor_pos: chat.last_cursor_pos,
                    from_uid,
                    reply_preview: text,
                });
            }
            Task::none()
        }

        AppMessage::DismissAttachmentMenu => {
            if let Some(chat) = &mut state.active_chat {
                chat.attachment_menu = None;
            }
            Task::none()
        }

        AppMessage::ReplyToMessagePressed {
            server_message_id,
            from_uid,
            preview,
        } => {
            if let Some(chat) = &mut state.active_chat {
                chat.attachment_menu = None;
                chat.composer.pending_reply = Some(crate::app::state::PendingReplyState {
                    server_message_id,
                    from_uid,
                    preview,
                });
            }
            Task::none()
        }

        AppMessage::CancelPendingReply => {
            if let Some(chat) = &mut state.active_chat {
                chat.composer.pending_reply = None;
            }
            Task::none()
        }

        AppMessage::ChatCursorMoved(point) => {
            if let Some(chat) = &mut state.active_chat {
                chat.last_cursor_pos = Some(point);
            }
            Task::none()
        }

        AppMessage::TextMenuCopy => {
            let text_to_copy = state
                .active_chat
                .as_ref()
                .and_then(|chat| chat.attachment_menu.as_ref())
                .and_then(|menu| menu.copy_text.clone());
            if let Some(chat) = &mut state.active_chat {
                chat.attachment_menu = None;
            }
            match text_to_copy {
                Some(value) => match copy_text_to_clipboard(&value) {
                    Ok(()) => Task::none(),
                    Err(error) => {
                        state.auth.error = Some(format!("复制失败: {}", format_ui_error(&error)));
                        Task::none()
                    }
                },
                None => Task::none(),
            }
        }

        AppMessage::AttachmentMenuOpen => {
            let menu = state
                .active_chat
                .as_ref()
                .and_then(|chat| chat.attachment_menu.clone());
            if let Some(chat) = &mut state.active_chat {
                chat.attachment_menu = None;
            }
            match menu {
                Some(menu) => {
                    if let Some(local) = menu.local_path {
                        let path = Path::new(&local);
                        if path.exists() {
                            return match reveal_in_file_manager(&local) {
                                Ok(()) => Task::none(),
                                Err(error) => Task::done(AppMessage::AttachmentOpenResolved {
                                    result: Err(error),
                                }),
                            };
                        }
                    }
                    let Some(file_id) = menu.file_id else {
                        return Task::done(AppMessage::AttachmentOpenResolved {
                            result: Err(crate::presentation::vm::UiError::Unknown(
                                "attachment file_id missing".to_string(),
                            )),
                        });
                    };
                    let bridge = Arc::clone(bridge);
                    let filename = menu.filename.clone();
                    let message_id = menu.message_id;
                    let created_at = menu.created_at;

                    Task::perform(
                        async move {
                            let url = bridge.get_file_url(file_id).await?;
                            let path = bridge
                                .download_message_media_blocking(
                                    message_id,
                                    url,
                                    String::new(),
                                    Some(filename),
                                    created_at,
                                )
                                .await?;
                            reveal_in_file_manager(&path)?;
                            Ok(path)
                        },
                        |result| AppMessage::AttachmentOpenResolved { result },
                    )
                }
                None => Task::none(),
            }
        }

        AppMessage::AttachmentMenuOpenFolder => {
            let menu = state
                .active_chat
                .as_ref()
                .and_then(|chat| chat.attachment_menu.clone());
            if let Some(chat) = &mut state.active_chat {
                chat.attachment_menu = None;
            }
            match menu {
                Some(menu) => {
                    if let Some(local) = menu.local_path {
                        let path = Path::new(&local);
                        if path.exists() {
                            let parent = path
                                .parent()
                                .map(|p| p.to_string_lossy().to_string())
                                .unwrap_or(local);
                            return match open_with_system(&parent) {
                                Ok(()) => Task::none(),
                                Err(error) => {
                                    Task::done(AppMessage::AttachmentOpenFolderResolved {
                                        result: Err(error),
                                    })
                                }
                            };
                        }
                    }
                    let Some(file_id) = menu.file_id else {
                        return Task::done(AppMessage::AttachmentOpenFolderResolved {
                            result: Err(crate::presentation::vm::UiError::Unknown(
                                "attachment file_id missing".to_string(),
                            )),
                        });
                    };
                    let bridge = Arc::clone(bridge);
                    let filename = menu.filename.clone();
                    let message_id = menu.message_id;
                    let created_at = menu.created_at;

                    Task::perform(
                        async move {
                            let url = bridge.get_file_url(file_id).await?;
                            let path = bridge
                                .download_message_media_blocking(
                                    message_id,
                                    url,
                                    String::new(),
                                    Some(filename),
                                    created_at,
                                )
                                .await?;
                            let parent = Path::new(&path)
                                .parent()
                                .map(|p| p.to_string_lossy().to_string())
                                .unwrap_or(path.clone());
                            open_with_system(&parent)?;
                            Ok(parent)
                        },
                        |result| AppMessage::AttachmentOpenFolderResolved { result },
                    )
                }
                None => Task::none(),
            }
        }

        AppMessage::AttachmentMenuSaveAs => {
            let menu = state
                .active_chat
                .as_ref()
                .and_then(|chat| chat.attachment_menu.clone());
            if let Some(chat) = &mut state.active_chat {
                chat.attachment_menu = None;
            }
            match menu {
                Some(menu) => {
                    let message_id = menu.message_id;
                    let created_at = menu.created_at;
                    let local_path = menu.local_path;
                    let file_id = menu.file_id;
                    let filename = menu.filename.clone();
                    let dialog_filename = filename.clone();
                    Task::perform(
                        async move {
                            rfd::FileDialog::new()
                                .set_file_name(&dialog_filename)
                                .save_file()
                                .map(|path| path.to_string_lossy().to_string())
                        },
                        move |save_path| AppMessage::AttachmentSaveAsSelected {
                            message_id,
                            created_at,
                            local_path,
                            file_id,
                            filename,
                            save_path,
                        },
                    )
                }
                None => Task::none(),
            }
        }

        AppMessage::AttachmentSaveAsSelected {
            message_id,
            created_at,
            local_path,
            file_id,
            filename,
            save_path,
        } => match save_path {
            Some(path) => {
                if let Some(local) = local_path {
                    let src = Path::new(&local);
                    if src.exists() {
                        return match fs::copy(src, &path) {
                            Ok(_) => Task::done(AppMessage::AttachmentSaveAsResolved {
                                result: Ok(path),
                            }),
                            Err(error) => Task::done(AppMessage::AttachmentSaveAsResolved {
                                result: Err(crate::presentation::vm::UiError::Unknown(format!(
                                    "copy failed: {error}"
                                ))),
                            }),
                        };
                    }
                }
                let Some(file_id) = file_id else {
                    return Task::done(AppMessage::AttachmentSaveAsResolved {
                        result: Err(crate::presentation::vm::UiError::Unknown(
                            "attachment file_id missing".to_string(),
                        )),
                    });
                };
                let bridge = Arc::clone(bridge);

                Task::perform(
                    async move {
                        let url = bridge.get_file_url(file_id).await?;
                        let canonical = bridge
                            .download_message_media_blocking(
                                message_id,
                                url,
                                String::new(),
                                Some(filename),
                                created_at,
                            )
                            .await?;
                        fs::copy(&canonical, &path).map_err(|error| {
                            crate::presentation::vm::UiError::Unknown(format!(
                                "copy to save_as target failed: {error}"
                            ))
                        })?;
                        Ok(path)
                    },
                    |result| AppMessage::AttachmentSaveAsResolved { result },
                )
            }
            None => Task::none(),
        },

        AppMessage::AttachmentOpenResolved { result } => {
            if let Err(error) = result {
                warn!("open attachment failed: {}", format_ui_error(&error));
            }
            Task::none()
        }

        AppMessage::AttachmentOpenFolderResolved { result } => {
            if let Err(error) = result {
                warn!("open attachment folder failed: {}", format_ui_error(&error));
            }
            Task::none()
        }

        AppMessage::AttachmentSaveAsResolved { result } => {
            if let Err(error) = result {
                warn!("save attachment as failed: {}", format_ui_error(&error));
            }
            Task::none()
        }

        AppMessage::CloseImagePreview => {
            if let Some(wid) = state.image_viewer_window_id.take() {
                state.image_viewer = None;
                return window::close(wid);
            }
            Task::none()
        }

        AppMessage::OpenExternalUrl(url) => {
            // 仅放行 http/https，避免误用 file:// 等本地协议触发命令注入。
            let trimmed = url.trim().to_string();
            let scheme_ok = trimmed.starts_with("http://") || trimmed.starts_with("https://");
            if scheme_ok && !trimmed.is_empty() {
                if let Err(error) = open_with_system(&trimmed) {
                    warn!(
                        "open external url failed: url={} error={}",
                        trimmed,
                        format_ui_error(&error)
                    );
                }
            }
            Task::none()
        }

        AppMessage::OpenUserProfile { user_id } => {
            tracing::info!(
                "OpenUserProfile: user_id={} peer_user_id={:?} channel_id={:?}",
                user_id,
                state.active_chat.as_ref().map(|c| c.peer_user_id),
                state.active_chat.as_ref().map(|c| c.channel_id),
            );
            let fallback_name = state
                .active_chat
                .as_ref()
                .map(|c| c.title.clone());
            let channel_id = state
                .active_chat
                .as_ref()
                .map(|c| c.channel_id)
                .unwrap_or(0);
            if let Some(chat) = &mut state.active_chat {
                chat.user_profile_panel = Some(crate::app::state::UserProfilePanelState {
                    user_id,
                    loading: true,
                    detail: None,
                    error: None,
                    editing_alias: false,
                    alias_input: String::new(),
                    alias_old_title: None,
                });
            }
            let bridge = bridge.clone();
            Task::perform(
                async move { bridge.load_user_profile(user_id, channel_id, fallback_name).await },
                move |result| match result {
                    Ok(detail) => AppMessage::UserProfileLoaded { user_id, detail },
                    Err(error) => AppMessage::UserProfileLoadFailed { user_id, error },
                },
            )
        }

        AppMessage::UserProfileLoaded { user_id, detail } => {
            if let Some(chat) = &mut state.active_chat {
                if let Some(panel) = &mut chat.user_profile_panel {
                    if panel.user_id == user_id {
                        panel.loading = false;
                        panel.detail = Some(detail);
                    }
                }
            }
            Task::none()
        }

        AppMessage::UserProfileLoadFailed { user_id, error } => {
            if let Some(chat) = &mut state.active_chat {
                if let Some(panel) = &mut chat.user_profile_panel {
                    if panel.user_id == user_id {
                        panel.loading = false;
                        panel.error = Some(format!("{error:?}"));
                    }
                }
            }
            Task::none()
        }

        AppMessage::CloseUserProfile => {
            if let Some(chat) = &mut state.active_chat {
                chat.user_profile_panel = None;
            }
            Task::none()
        }

        AppMessage::StartEditAlias => {
            if let Some(chat) = &mut state.active_chat {
                if let Some(panel) = &mut chat.user_profile_panel {
                    // Pre-fill with current title (display name)
                    let current = panel
                        .detail
                        .as_ref()
                        .map(|d| d.title.clone())
                        .unwrap_or_default();
                    panel.alias_input = current;
                    panel.editing_alias = true;
                }
            }
            Task::none()
        }

        AppMessage::AliasInputChanged(value) => {
            if let Some(chat) = &mut state.active_chat {
                if let Some(panel) = &mut chat.user_profile_panel {
                    panel.alias_input = value;
                }
            }
            Task::none()
        }

        AppMessage::CancelEditAlias => {
            if let Some(chat) = &mut state.active_chat {
                if let Some(panel) = &mut chat.user_profile_panel {
                    panel.editing_alias = false;
                    panel.alias_input.clear();
                }
            }
            Task::none()
        }

        AppMessage::ConfirmEditAlias => {
            if let Some(chat) = &mut state.active_chat {
                if let Some(panel) = &mut chat.user_profile_panel {
                    let alias = panel.alias_input.trim().to_string();
                    let user_id = panel.user_id;
                    panel.editing_alias = false;
                    panel.error = None;
                    // Save old title for rollback on failure
                    panel.alias_old_title = panel.detail.as_ref().map(|d| d.title.clone());
                    // Optimistic update
                    if !alias.is_empty() {
                        if let Some(detail) = &mut panel.detail {
                            detail.title = alias.clone();
                        }
                    }
                    let bridge = bridge.clone();
                    let alias_clone = alias.clone();
                    return Task::perform(
                        async move { bridge.set_friend_alias(user_id, alias).await },
                        move |result| match result {
                            Ok(success) => AppMessage::AliasSetResult { success, alias: alias_clone.clone() },
                            Err(_) => AppMessage::AliasSetResult { success: false, alias: alias_clone.clone() },
                        },
                    );
                }
            }
            Task::none()
        }

        AppMessage::AliasSetResult { success, alias } => {
            // Collect info needed for session list update before borrowing active_chat
            let mut update_session: Option<(u64, i32, String)> = None;

            if let Some(chat) = &mut state.active_chat {
                if let Some(panel) = &mut chat.user_profile_panel {
                    if !success {
                        // Rollback profile card title
                        if let Some(old_title) = panel.alias_old_title.take() {
                            if let Some(detail) = &mut panel.detail {
                                detail.title = old_title;
                            }
                        }
                        panel.error = Some("设置失败，请稍后再试。".to_string());
                    } else {
                        panel.alias_old_title = None;
                        let new_title = if alias.is_empty() {
                            panel.detail.as_ref().map(|d| d.title.clone()).unwrap_or_default()
                        } else {
                            alias
                        };
                        // Update chat header title
                        chat.title = new_title.clone();
                        update_session = Some((chat.channel_id, chat.channel_type, new_title));
                    }
                }
            }

            // Update session list title
            if let Some((channel_id, channel_type, new_title)) = update_session {
                if let Some(item) = state.session_list.items.iter_mut().find(|item| {
                    item.channel_id == channel_id && item.channel_type == channel_type
                }) {
                    item.title = new_title;
                }
            }

            Task::none()
        }

        AppMessage::MediaThumbnailDownloaded {
            message_id,
            local_path,
        } => {
            state.media_downloads_inflight.remove(&message_id);
            if let Some(chat) = &mut state.active_chat {
                if let Some(item) = chat
                    .timeline
                    .items
                    .iter_mut()
                    .find(|item| item.message_id == message_id)
                {
                    if is_thumbnail_local_path(&local_path) {
                        item.local_thumbnail_path = Some(local_path.clone());
                    } else {
                        item.media_local_path = Some(local_path.clone());
                    }
                }
            }
            // Trigger async decode for the newly downloaded image
            let decode_tasks = schedule_image_decodes(state);
            if !decode_tasks.is_empty() {
                return Task::batch(decode_tasks);
            }
            Task::none()
        }

        AppMessage::MediaThumbnailDownloadFailed { message_id, error } => {
            // Keep failed id in the inflight/attempted set to avoid hot-loop retries
            // on every session refresh / patch tick. Users can still open attachment
            // explicitly to trigger an on-demand fetch.
            warn!(
                "media thumbnail download failed: message_id={} error={}",
                message_id,
                format_ui_error(&error)
            );
            Task::none()
        }

        AppMessage::ImageDecoded { message_id, handle } => {
            state.image_decode_pending.remove(&message_id);
            state.image_cache.insert(message_id, handle);
            Task::none()
        }

        AppMessage::ImageDecodeFailed { message_id } => {
            state.image_decode_pending.remove(&message_id);
            Task::none()
        }

        AppMessage::ComposerEdited { action } => {
            if let Some(chat) = &mut state.active_chat {
                let old_draft = chat.composer.draft.clone();
                chat.composer.editor.perform(action);
                let raw_new = chat.composer.editor.text();
                // diff 合并：若编辑咬到了某个 span 的区间，把整段 span 从旧文本中摘掉。
                let (merged, survivors) = crate::app::state::resolve_mention_edit(
                    &old_draft,
                    &raw_new,
                    &chat.composer.mentions,
                );
                // 当 resolve 决定原子删除时，merged 与 raw_new 不同 —— 需要把 editor
                // 的内容改回 merged（iced 的 text_editor 没有直接 set API，只能重建 Content）。
                if merged != raw_new {
                    chat.composer.editor =
                        iced::widget::text_editor::Content::with_text(&merged);
                }
                chat.composer.draft = merged;
                chat.composer.mentions = survivors;

                let is_dm = chat.channel_type == 1;
                let picker_state = if is_dm {
                    None
                } else {
                    match crate::app::state::compute_mention_query(
                        &chat.composer.draft,
                        is_dm,
                    ) {
                        Some(query) => {
                            let filtered: Vec<_> = chat
                                .composer
                                .group_members
                                .iter()
                                .filter(|m| {
                                    crate::app::state::match_member_query(m, &query)
                                })
                                .cloned()
                                .collect();
                            Some(crate::app::state::MentionPickerState { query, filtered })
                        }
                        None => None,
                    }
                };
                chat.composer.mention_picker = picker_state;

                let is_typing = !chat.composer.draft.trim().is_empty();
                chat.composer.typing_active = is_typing;
                return schedule_send_typing_task(
                    bridge,
                    chat.channel_id,
                    chat.channel_type,
                    is_typing,
                );
            }
            Task::none()
        }

        AppMessage::MentionMembersLoaded { channel_id, members } => {
            if let Some(chat) = &mut state.active_chat {
                if chat.channel_id == channel_id {
                    chat.composer.group_members = members;
                }
            }
            Task::none()
        }

        AppMessage::MentionPickerPicked { user_id } => {
            if let Some(chat) = &mut state.active_chat {
                if chat.channel_type != 1 {
                    if let Some(member) = chat
                        .composer
                        .group_members
                        .iter()
                        .find(|m| m.user_id == user_id)
                        .cloned()
                    {
                        let label = {
                            let l = member.best_label();
                            if l.is_empty() {
                                format!("用户{}", user_id)
                            } else {
                                l.to_string()
                            }
                        };
                        let (new_text, span) = crate::app::state::replace_mention_query(
                            &chat.composer.draft,
                            &label,
                            user_id,
                        );
                        chat.composer.editor =
                            iced::widget::text_editor::Content::with_text(&new_text);
                        chat.composer.draft = new_text;
                        chat.composer.mentions.push(span);
                        chat.composer.mention_picker = None;
                    }
                }
            }
            Task::none()
        }

        AppMessage::MentionPickerDismissed => {
            if let Some(chat) = &mut state.active_chat {
                chat.composer.mention_picker = None;
            }
            Task::none()
        }

        AppMessage::SendPressed => handle_send_pressed(state, bridge),

        AppMessage::RetrySendPressed {
            channel_id,
            channel_type,
            client_txn_id,
        } => handle_retry_send_pressed(state, bridge, channel_id, channel_type, client_txn_id),

        AppMessage::RevokeMessagePressed {
            channel_id,
            channel_type: _,
            server_message_id,
        } => {
            if let Some(chat) = &mut state.active_chat {
                chat.attachment_menu = None;
            }
            let bridge = Arc::clone(bridge);
            Task::perform(
                async move { bridge.revoke_message(channel_id, server_message_id).await },
                move |result| match result {
                    Ok(_) => AppMessage::RevokeMessageSucceeded { server_message_id },
                    Err(error) => AppMessage::RevokeMessageFailed {
                        server_message_id,
                        error,
                    },
                },
            )
        }

        AppMessage::RevokeMessageSucceeded { server_message_id } => {
            if let Some(chat) = &mut state.active_chat {
                if let Some(item) = chat
                    .timeline
                    .items
                    .iter_mut()
                    .find(|item| item.server_message_id == Some(server_message_id))
                {
                    item.is_deleted = true;
                }
            }
            schedule_session_list_refresh(state, bridge)
        }

        AppMessage::RevokeMessageFailed {
            server_message_id,
            error,
        } => {
            if is_already_revoked_error(&error) {
                if let Some(chat) = &mut state.active_chat {
                    if let Some(item) = chat
                        .timeline
                        .items
                        .iter_mut()
                        .find(|item| item.server_message_id == Some(server_message_id))
                    {
                        item.is_deleted = true;
                    }
                }
                return schedule_session_list_refresh(state, bridge);
            }
            let error_text = format_ui_error(&error);
            let ui_text = if is_revoke_timeout_error(&error) {
                "撤回失败：消息发送超过 2 分钟，服务器已拒绝撤回".to_string()
            } else {
                format!("撤回失败：{error_text}")
            };
            warn!(
                "revoke message failed: server_message_id={} error={}",
                server_message_id, error_text
            );
            append_runtime_log(state, "WARN", &ui_text);
            state.auth.error = Some(ui_text);
            Task::none()
        }

        AppMessage::RequestDeleteMessageLocal {
            channel_id,
            channel_type,
            open_token,
            message_id,
            key,
            preview,
        } => {
            if let Some(chat) = &mut state.active_chat {
                chat.attachment_menu = None;
                chat.delete_confirm = Some(crate::app::state::DeleteConfirmState {
                    channel_id,
                    channel_type,
                    open_token,
                    message_id,
                    message_key: key,
                    preview,
                });
            }
            Task::none()
        }

        AppMessage::CancelDeleteMessageLocal => {
            if let Some(chat) = &mut state.active_chat {
                chat.delete_confirm = None;
            }
            Task::none()
        }

        AppMessage::DeleteMessageLocalPressed {
            channel_id,
            channel_type,
            open_token,
            message_id,
            key,
        } => {
            if let Some(chat) = &mut state.active_chat {
                chat.attachment_menu = None;
                chat.delete_confirm = None;
            }
            let bridge = Arc::clone(bridge);
            Task::perform(
                async move { bridge.delete_message_local(message_id).await },
                move |result| AppMessage::DeleteMessageLocalResolved {
                    channel_id,
                    channel_type,
                    open_token,
                    key: key.clone(),
                    result,
                },
            )
        }

        AppMessage::DeleteMessageLocalResolved {
            channel_id,
            channel_type,
            open_token,
            key,
            result,
        } => match result {
            Ok(_) => Task::done(AppMessage::TimelinePatched {
                channel_id,
                channel_type,
                open_token,
                revision: events::allocate_patch_revision(),
                patch: TimelinePatchVm::RemoveMessage { key },
            }),
            Err(error) => {
                let text = format!("本地删除失败：{}", format_ui_error(&error));
                warn!("delete_message_local failed: {}", format_ui_error(&error));
                append_runtime_log(state, "WARN", &text);
                state.auth.error = Some(text);
                Task::none()
            }
        },

        AppMessage::ToggleReactionPicker { message_id } => {
            if let Some(chat) = &mut state.active_chat {
                chat.attachment_menu = None;
                chat.reaction_picker_for = match (chat.reaction_picker_for, message_id) {
                    (Some(current), Some(next)) if current == next => None,
                    (_, next) => next,
                };
            }
            Task::none()
        }

        AppMessage::ToggleReactionPressed {
            channel_id,
            channel_type,
            open_token,
            message_id,
            server_message_id,
            emoji,
            currently_mine,
        } => {
            let Some(my_uid) = state.auth.user_id else {
                return Task::none();
            };
            if let Some(chat) = &mut state.active_chat {
                chat.reaction_picker_for = None;
            }
            let was_add = !currently_mine;
            let bridge_arc = Arc::clone(bridge);
            let emoji_clone = emoji.clone();
            Task::perform(
                async move {
                    if was_add {
                        bridge_arc
                            .add_reaction(
                                server_message_id,
                                channel_id,
                                channel_type,
                                message_id,
                                my_uid,
                                emoji_clone,
                            )
                            .await
                    } else {
                        bridge_arc
                            .remove_reaction(
                                server_message_id,
                                channel_id,
                                channel_type,
                                message_id,
                                my_uid,
                                emoji_clone,
                            )
                            .await
                    }
                },
                move |result| AppMessage::ReactionToggleResolved {
                    channel_id,
                    channel_type,
                    open_token,
                    message_id,
                    emoji: emoji.clone(),
                    was_add,
                    result,
                },
            )
        }

        AppMessage::ReactionToggleResolved {
            channel_id,
            channel_type,
            open_token,
            message_id,
            emoji,
            was_add,
            result,
        } => match result {
            Ok(()) => Task::done(AppMessage::RequestReactionsRefresh {
                channel_id,
                channel_type,
                open_token,
            }),
            Err(error) => {
                let verb = if was_add { "添加" } else { "移除" };
                let err_text = format_ui_error(&error);
                warn!(
                    "reaction toggle failed: message_id={} emoji={} verb={} error={}",
                    message_id, emoji, verb, err_text
                );
                let text = format!("反应{verb}失败：{err_text}");
                append_runtime_log(state, "WARN", &text);
                state.auth.error = Some(text);
                Task::none()
            }
        },

        AppMessage::RequestReactionsRefresh {
            channel_id,
            channel_type,
            open_token,
        } => {
            let Some(chat) = state.active_chat.as_ref() else {
                return Task::none();
            };
            if chat.channel_id != channel_id
                || chat.channel_type != channel_type
                || chat.open_token != open_token
            {
                return Task::none();
            }
            let message_ids: Vec<u64> = chat
                .timeline
                .items
                .iter()
                .filter_map(|m| m.server_message_id.map(|_| m.message_id))
                .collect();
            if message_ids.is_empty() {
                return Task::none();
            }
            let my_uid = state.auth.user_id.unwrap_or(0);
            let bridge_arc = Arc::clone(bridge);
            Task::perform(
                async move {
                    bridge_arc
                        .list_local_message_reactions_batch(message_ids, my_uid)
                        .await
                },
                move |result| match result {
                    Ok(map) => AppMessage::ReactionsBatchLoaded {
                        channel_id,
                        channel_type,
                        open_token,
                        map,
                    },
                    Err(error) => {
                        warn!(
                            "list_local_message_reactions_batch failed: {}",
                            format_ui_error(&error)
                        );
                        AppMessage::Noop
                    }
                },
            )
        }

        AppMessage::ReactionsBatchLoaded {
            channel_id,
            channel_type,
            open_token,
            map,
        } => {
            if let Some(chat) = &mut state.active_chat {
                if chat.channel_id == channel_id
                    && chat.channel_type == channel_type
                    && chat.open_token == open_token
                {
                    chat.message_reactions = map;
                }
            }
            Task::none()
        }

        AppMessage::SyncMessageReactionChanged => {
            if let Some(chat) = state.active_chat.as_ref() {
                let channel_id = chat.channel_id;
                let channel_type = chat.channel_type;
                let open_token = chat.open_token;
                Task::done(AppMessage::RequestReactionsRefresh {
                    channel_id,
                    channel_type,
                    open_token,
                })
            } else {
                Task::none()
            }
        },

        AppMessage::OpenForwardPicker {
            channel_id,
            channel_type,
            message_id,
            server_message_id,
            preview,
        } => {
            let token = state.allocate_open_token();
            // 候选：从当前 session_list + add_friend 快照就地取。去重以 (channel_id)/uid 为键。
            let recent_sessions: Vec<_> = state
                .session_list
                .items
                .iter()
                .filter(|item| item.channel_id != channel_id || item.channel_type != channel_type)
                .cloned()
                .collect();
            let recent_dm_peers: HashSet<u64> = recent_sessions
                .iter()
                .filter(|s| s.channel_type == 1)
                .filter_map(|s| s.peer_user_id)
                .collect();
            let recent_group_channels: HashSet<u64> = recent_sessions
                .iter()
                .filter(|s| s.channel_type == 2)
                .map(|s| s.channel_id)
                .collect();
            let friends: Vec<_> = state
                .add_friend
                .friends
                .iter()
                .filter(|f| !recent_dm_peers.contains(&f.user_id))
                .cloned()
                .collect();
            let groups: Vec<_> = state
                .add_friend
                .groups
                .iter()
                .filter(|g| !recent_group_channels.contains(&g.group_id))
                .cloned()
                .collect();
            state.forward_picker = Some(ForwardPickerState {
                open_token: token,
                source_channel_id: channel_id,
                source_channel_type: channel_type,
                source_message_id: message_id,
                source_server_message_id: server_message_id,
                source_preview: preview,
                search: String::new(),
                recent_sessions,
                friends,
                groups,
                selected: Vec::new(),
                note: String::new(),
                submitting: false,
                error: None,
            });
            Task::none()
        }

        AppMessage::DismissForwardPicker => {
            state.forward_picker = None;
            Task::none()
        }

        AppMessage::ForwardSearchChanged(query) => {
            if let Some(picker) = &mut state.forward_picker {
                picker.search = query;
            }
            Task::none()
        }

        AppMessage::ForwardTargetToggled(target) => {
            if let Some(picker) = &mut state.forward_picker {
                if let Some(idx) = picker.selected.iter().position(|t| *t == target) {
                    picker.selected.remove(idx);
                    picker.error = None;
                } else if picker.selected.len() >= FORWARD_MAX_TARGETS {
                    picker.error = Some(format!("最多选择 {} 个目标", FORWARD_MAX_TARGETS));
                } else {
                    picker.selected.push(target);
                    picker.error = None;
                }
            }
            Task::none()
        }

        AppMessage::ForwardNoteChanged(text) => {
            if let Some(picker) = &mut state.forward_picker {
                let trimmed: String = text.chars().take(FORWARD_NOTE_MAX).collect();
                picker.note = trimmed;
            }
            Task::none()
        }

        AppMessage::ForwardSendPressed => {
            let Some(picker) = state.forward_picker.as_mut() else {
                return Task::none();
            };
            if picker.selected.is_empty() || picker.submitting {
                return Task::none();
            }
            picker.submitting = true;
            picker.error = None;
            let open_token = picker.open_token;
            let targets = picker.selected.clone();
            let note = picker.note.trim().to_string();
            let src_message_id = picker.source_message_id;
            let bridge_arc = Arc::clone(bridge);
            Task::perform(
                async move {
                    let mut success_count = 0usize;
                    let mut failures: Vec<String> = Vec::new();
                    for target in targets {
                        let resolved = match target {
                            ForwardTarget::DirectMessage(peer_uid) => {
                                match bridge_arc.get_or_create_direct_channel(peer_uid).await {
                                    Ok(pair) => Some(pair),
                                    Err(err) => {
                                        failures.push(format_ui_error(&err));
                                        None
                                    }
                                }
                            }
                            ForwardTarget::Group(group_id) => Some((group_id, 2)),
                        };
                        let Some((channel_id, channel_type)) = resolved else {
                            continue;
                        };
                        match bridge_arc
                            .forward_message(src_message_id, channel_id, channel_type)
                            .await
                        {
                            Ok(_) => {
                                success_count += 1;
                                if !note.is_empty() {
                                    match bridge_arc.generate_local_message_id() {
                                        Ok(client_txn_id) => {
                                            if let Err(err) = bridge_arc
                                                .send_text_message(
                                                    channel_id,
                                                    channel_type,
                                                    client_txn_id,
                                                    note.clone(),
                                                    None,
                                                    None,
                                                )
                                                .await
                                            {
                                                warn!(
                                                    "forward note send failed channel_id={} err={}",
                                                    channel_id,
                                                    format_ui_error(&err)
                                                );
                                            }
                                        }
                                        Err(err) => warn!(
                                            "forward note allocate id failed err={}",
                                            format_ui_error(&err)
                                        ),
                                    }
                                }
                            }
                            Err(err) => {
                                failures.push(format_ui_error(&err));
                            }
                        }
                    }
                    Ok::<ForwardSendSummary, UiError>(ForwardSendSummary {
                        success_count,
                        failures,
                    })
                },
                move |result| AppMessage::ForwardSendResolved { open_token, result },
            )
        }

        AppMessage::ForwardSendResolved { open_token, result } => {
            let Some(picker) = state.forward_picker.as_mut() else {
                return Task::none();
            };
            if picker.open_token != open_token {
                return Task::none();
            }
            picker.submitting = false;
            match result {
                Ok(summary) => {
                    if summary.failures.is_empty() {
                        state.forward_picker = None;
                    } else {
                        picker.error = Some(format!(
                            "部分目标失败 ({}): {}",
                            summary.failures.len(),
                            summary.failures.join("; ")
                        ));
                    }
                }
                Err(err) => {
                    picker.error = Some(format_ui_error(&err));
                }
            }
            Task::none()
        },

        AppMessage::OpenFriendSettings {
            user_id,
            title,
            avatar,
            remark,
        } => {
            let token = state.allocate_open_token();
            state.friend_settings = Some(FriendSettingsState {
                open_token: token,
                user_id,
                title,
                avatar,
                remark,
                direct_channel_id: None,
                loading: true,
                is_muted: false,
                is_blacklisted: false,
                editing_remark: false,
                remark_input: String::new(),
                submitting_remark: false,
                submitting_mute: false,
                submitting_block: false,
                delete_confirm_open: false,
                submitting_delete: false,
                error: None,
            });
            let bridge_arc = Arc::clone(bridge);
            Task::perform(
                async move {
                    let (direct_channel_id, _channel_type) =
                        bridge_arc.get_or_create_direct_channel(user_id).await?;
                    let is_blacklisted = bridge_arc.is_user_blacklisted(user_id).await?;
                    // mute 状态：读本地 SessionList 里对应 channel 的 mute 字段。
                    // 此处直接用 channel_id 为键从 session_list 查找会更省事，
                    // 但 bridge 没暴露单行查询，所以退而求其次在 update 侧依据 token 派送后再查。
                    Ok::<(u64, bool), UiError>((direct_channel_id, is_blacklisted))
                },
                move |result| match result {
                    Ok((direct_channel_id, is_blacklisted)) => AppMessage::FriendSettingsLoaded {
                        open_token: token,
                        direct_channel_id,
                        // mute 在 update 侧根据 session_list 填回，这里先占位 false。
                        is_muted: false,
                        is_blacklisted,
                    },
                    Err(error) => AppMessage::FriendSettingsLoadFailed {
                        open_token: token,
                        error,
                    },
                },
            )
        }

        AppMessage::DismissFriendSettings => {
            state.friend_settings = None;
            Task::none()
        }

        AppMessage::FriendSettingsLoaded {
            open_token,
            direct_channel_id,
            is_muted: _is_muted_placeholder,
            is_blacklisted,
        } => {
            let Some(panel) = state.friend_settings.as_mut() else {
                return Task::none();
            };
            if panel.open_token != open_token {
                return Task::none();
            }
            panel.loading = false;
            panel.direct_channel_id = Some(direct_channel_id);
            panel.is_blacklisted = is_blacklisted;
            // 用 session_list 里的 channel.mute 校准 mute 状态（bridge 加载只返回 channel_id，
            // 没返回 mute；SessionListItemVm 里有）。
            panel.is_muted = state
                .session_list
                .items
                .iter()
                .find(|item| item.channel_id == direct_channel_id && item.channel_type == 1)
                .map(|item| item.is_muted)
                .unwrap_or(false);
            Task::none()
        }

        AppMessage::FriendSettingsLoadFailed { open_token, error } => {
            let Some(panel) = state.friend_settings.as_mut() else {
                return Task::none();
            };
            if panel.open_token != open_token {
                return Task::none();
            }
            panel.loading = false;
            panel.error = Some(format_ui_error(&error));
            Task::none()
        }

        AppMessage::FriendSettingsRemarkEditPressed => {
            if let Some(panel) = state.friend_settings.as_mut() {
                panel.editing_remark = true;
                panel.remark_input = panel.remark.clone();
                panel.error = None;
            }
            Task::none()
        }

        AppMessage::FriendSettingsRemarkEditCancelled => {
            if let Some(panel) = state.friend_settings.as_mut() {
                panel.editing_remark = false;
                panel.remark_input.clear();
            }
            Task::none()
        }

        AppMessage::FriendSettingsRemarkInputChanged(text) => {
            if let Some(panel) = state.friend_settings.as_mut() {
                panel.remark_input = text;
            }
            Task::none()
        }

        AppMessage::FriendSettingsRemarkSubmitPressed => {
            let Some(panel) = state.friend_settings.as_mut() else {
                return Task::none();
            };
            if panel.submitting_remark {
                return Task::none();
            }
            let token = panel.open_token;
            let user_id = panel.user_id;
            let alias = panel.remark_input.trim().to_string();
            panel.submitting_remark = true;
            panel.error = None;
            let bridge_arc = Arc::clone(bridge);
            let alias_clone = alias.clone();
            Task::perform(
                async move {
                    let ok = bridge_arc.set_friend_alias(user_id, alias_clone).await?;
                    if !ok {
                        return Err(UiError::Unknown("设置备注失败".to_string()));
                    }
                    Ok::<String, UiError>(alias)
                },
                move |result| AppMessage::FriendSettingsRemarkResolved {
                    open_token: token,
                    result,
                },
            )
        }

        AppMessage::FriendSettingsRemarkResolved { open_token, result } => {
            let Some(panel) = state.friend_settings.as_mut() else {
                return Task::none();
            };
            if panel.open_token != open_token {
                return Task::none();
            }
            panel.submitting_remark = false;
            match result {
                Ok(alias) => {
                    panel.editing_remark = false;
                    panel.remark = alias.clone();
                    // title 反映 remark > nickname：有备注时优先展示备注；
                    // 清空时退回 remark_input 里原先的显示名不一定可得，这里保留 title 不覆盖。
                    if !alias.is_empty() {
                        panel.title = alias;
                    }
                    panel.remark_input.clear();
                }
                Err(err) => {
                    panel.error = Some(format_ui_error(&err));
                }
            }
            Task::none()
        }

        AppMessage::FriendSettingsMuteToggled(muted) => {
            let Some(panel) = state.friend_settings.as_mut() else {
                return Task::none();
            };
            if panel.submitting_mute {
                return Task::none();
            }
            let Some(channel_id) = panel.direct_channel_id else {
                panel.error = Some("私聊频道尚未就绪".to_string());
                return Task::none();
            };
            let token = panel.open_token;
            panel.submitting_mute = true;
            panel.error = None;
            // 乐观更新。
            panel.is_muted = muted;
            let bridge_arc = Arc::clone(bridge);
            Task::perform(
                async move {
                    bridge_arc.mute_channel(channel_id, muted).await
                },
                move |result| AppMessage::FriendSettingsMuteResolved {
                    open_token: token,
                    muted,
                    result,
                },
            )
        }

        AppMessage::FriendSettingsMuteResolved {
            open_token,
            muted,
            result,
        } => {
            let Some(panel) = state.friend_settings.as_mut() else {
                return Task::none();
            };
            if panel.open_token != open_token {
                return Task::none();
            }
            panel.submitting_mute = false;
            match result {
                Ok(()) => {
                    // 同步本地 session_list 的 mute 字段，避免用户返回后看到旧值。
                    if let Some(channel_id) = panel.direct_channel_id {
                        if let Some(item) = state
                            .session_list
                            .items
                            .iter_mut()
                            .find(|i| i.channel_id == channel_id && i.channel_type == 1)
                        {
                            item.is_muted = muted;
                        }
                    }
                }
                Err(err) => {
                    // 回滚。
                    panel.is_muted = !muted;
                    panel.error = Some(format_ui_error(&err));
                }
            }
            Task::none()
        }

        AppMessage::FriendSettingsBlockToggled(blocked) => {
            let Some(panel) = state.friend_settings.as_mut() else {
                return Task::none();
            };
            if panel.submitting_block {
                return Task::none();
            }
            let token = panel.open_token;
            let user_id = panel.user_id;
            panel.submitting_block = true;
            panel.error = None;
            panel.is_blacklisted = blocked;
            let bridge_arc = Arc::clone(bridge);
            Task::perform(
                async move {
                    if blocked {
                        bridge_arc.add_to_blacklist(user_id).await
                    } else {
                        bridge_arc.remove_from_blacklist(user_id).await
                    }
                },
                move |result| AppMessage::FriendSettingsBlockResolved {
                    open_token: token,
                    blocked,
                    result,
                },
            )
        }

        AppMessage::FriendSettingsBlockResolved {
            open_token,
            blocked,
            result,
        } => {
            let Some(panel) = state.friend_settings.as_mut() else {
                return Task::none();
            };
            if panel.open_token != open_token {
                return Task::none();
            }
            panel.submitting_block = false;
            if let Err(err) = result {
                panel.is_blacklisted = !blocked;
                panel.error = Some(format_ui_error(&err));
            }
            Task::none()
        }

        AppMessage::FriendSettingsDeletePressed => {
            if let Some(panel) = state.friend_settings.as_mut() {
                panel.delete_confirm_open = true;
                panel.error = None;
            }
            Task::none()
        }

        AppMessage::FriendSettingsDeleteCancelled => {
            if let Some(panel) = state.friend_settings.as_mut() {
                panel.delete_confirm_open = false;
            }
            Task::none()
        }

        AppMessage::FriendSettingsDeleteConfirmed => {
            let Some(panel) = state.friend_settings.as_mut() else {
                return Task::none();
            };
            if panel.submitting_delete {
                return Task::none();
            }
            let token = panel.open_token;
            let user_id = panel.user_id;
            panel.submitting_delete = true;
            panel.error = None;
            let bridge_arc = Arc::clone(bridge);
            Task::perform(
                async move { bridge_arc.delete_friend(user_id).await },
                move |result| AppMessage::FriendSettingsDeleteResolved {
                    open_token: token,
                    result,
                },
            )
        }

        AppMessage::FriendSettingsDeleteResolved { open_token, result } => {
            let Some(panel) = state.friend_settings.as_mut() else {
                return Task::none();
            };
            if panel.open_token != open_token {
                return Task::none();
            }
            panel.submitting_delete = false;
            match result {
                Ok(()) => {
                    // 删除成功，关闭面板；好友列表在下次拉取时会刷新。
                    state.friend_settings = None;
                }
                Err(err) => {
                    panel.delete_confirm_open = false;
                    panel.error = Some(format_ui_error(&err));
                }
            }
            Task::none()
        }

        AppMessage::OpenGroupSettings { group_id, title } => {
            let token = state.allocate_open_token();
            let my_user_id = state.auth.user_id.unwrap_or(0);
            state.group_settings = Some(GroupSettingsState {
                open_token: token,
                group_id,
                title,
                my_user_id,
                loading: true,
                members: Vec::new(),
                my_role: None,
                invite_input: String::new(),
                submitting_invite: false,
                submitting_remove: None,
                leave_confirm_open: false,
                submitting_leave: false,
                error: None,
            });
            let bridge_arc = Arc::clone(bridge);
            Task::perform(
                async move {
                    let members = bridge_arc.fetch_group_members_detailed(group_id).await?;
                    let my_role = members
                        .iter()
                        .find(|m| m.user_id == my_user_id)
                        .map(|m| m.role.clone());
                    Ok::<_, UiError>((members, my_role))
                },
                move |result| match result {
                    Ok((members, my_role)) => AppMessage::GroupSettingsLoaded {
                        open_token: token,
                        members,
                        my_role,
                    },
                    Err(error) => AppMessage::GroupSettingsLoadFailed {
                        open_token: token,
                        error,
                    },
                },
            )
        }

        AppMessage::DismissGroupSettings => {
            state.group_settings = None;
            Task::none()
        }

        AppMessage::GroupSettingsLoaded {
            open_token,
            members,
            my_role,
        } => {
            let Some(panel) = state.group_settings.as_mut() else {
                return Task::none();
            };
            if panel.open_token != open_token {
                return Task::none();
            }
            panel.loading = false;
            panel.members = members;
            panel.my_role = my_role;
            Task::none()
        }

        AppMessage::GroupSettingsLoadFailed { open_token, error } => {
            let Some(panel) = state.group_settings.as_mut() else {
                return Task::none();
            };
            if panel.open_token != open_token {
                return Task::none();
            }
            panel.loading = false;
            panel.error = Some(format_ui_error(&error));
            Task::none()
        }

        AppMessage::GroupSettingsInviteInputChanged(text) => {
            if let Some(panel) = state.group_settings.as_mut() {
                panel.invite_input = text;
            }
            Task::none()
        }

        AppMessage::GroupSettingsInviteSubmitPressed => {
            let Some(panel) = state.group_settings.as_mut() else {
                return Task::none();
            };
            if panel.submitting_invite {
                return Task::none();
            }
            let Ok(invited_user_id) = panel.invite_input.trim().parse::<u64>() else {
                panel.error = Some("请输入合法的用户 ID（数字）".to_string());
                return Task::none();
            };
            if invited_user_id == 0 {
                panel.error = Some("用户 ID 不能为 0".to_string());
                return Task::none();
            }
            if panel.members.iter().any(|m| m.user_id == invited_user_id) {
                panel.error = Some("该用户已在群内".to_string());
                return Task::none();
            }
            let token = panel.open_token;
            let group_id = panel.group_id;
            panel.submitting_invite = true;
            panel.error = None;
            let bridge_arc = Arc::clone(bridge);
            Task::perform(
                async move { bridge_arc.add_group_member(group_id, invited_user_id).await },
                move |result| AppMessage::GroupSettingsInviteResolved {
                    open_token: token,
                    invited_user_id,
                    result,
                },
            )
        }

        AppMessage::GroupSettingsInviteResolved {
            open_token,
            invited_user_id: _,
            result,
        } => {
            let Some(panel) = state.group_settings.as_mut() else {
                return Task::none();
            };
            if panel.open_token != open_token {
                return Task::none();
            }
            panel.submitting_invite = false;
            match result {
                Ok(()) => {
                    panel.invite_input.clear();
                    // 重新拉取成员列表以同步 role / joined_at。
                    let token = panel.open_token;
                    let group_id = panel.group_id;
                    let my_user_id = panel.my_user_id;
                    let bridge_arc = Arc::clone(bridge);
                    return Task::perform(
                        async move {
                            let members =
                                bridge_arc.fetch_group_members_detailed(group_id).await?;
                            let my_role = members
                                .iter()
                                .find(|m| m.user_id == my_user_id)
                                .map(|m| m.role.clone());
                            Ok::<_, UiError>((members, my_role))
                        },
                        move |result| match result {
                            Ok((members, my_role)) => AppMessage::GroupSettingsLoaded {
                                open_token: token,
                                members,
                                my_role,
                            },
                            Err(error) => AppMessage::GroupSettingsLoadFailed {
                                open_token: token,
                                error,
                            },
                        },
                    );
                }
                Err(err) => {
                    panel.error = Some(format_ui_error(&err));
                }
            }
            Task::none()
        }

        AppMessage::GroupSettingsRemoveMemberPressed(user_id) => {
            let Some(panel) = state.group_settings.as_mut() else {
                return Task::none();
            };
            if panel.submitting_remove.is_some() {
                return Task::none();
            }
            if user_id == panel.my_user_id {
                panel.error = Some("不能移除自己，请使用「退出群组」".to_string());
                return Task::none();
            }
            if !panel.is_admin() {
                panel.error = Some("仅群主或管理员可移除成员".to_string());
                return Task::none();
            }
            let token = panel.open_token;
            let group_id = panel.group_id;
            panel.submitting_remove = Some(user_id);
            panel.error = None;
            let bridge_arc = Arc::clone(bridge);
            Task::perform(
                async move { bridge_arc.remove_group_member(group_id, user_id).await },
                move |result| AppMessage::GroupSettingsRemoveMemberResolved {
                    open_token: token,
                    user_id,
                    result,
                },
            )
        }

        AppMessage::GroupSettingsRemoveMemberResolved {
            open_token,
            user_id,
            result,
        } => {
            let Some(panel) = state.group_settings.as_mut() else {
                return Task::none();
            };
            if panel.open_token != open_token {
                return Task::none();
            }
            panel.submitting_remove = None;
            match result {
                Ok(()) => {
                    panel.members.retain(|m| m.user_id != user_id);
                }
                Err(err) => {
                    panel.error = Some(format_ui_error(&err));
                }
            }
            Task::none()
        }

        AppMessage::GroupSettingsLeavePressed => {
            if let Some(panel) = state.group_settings.as_mut() {
                panel.leave_confirm_open = true;
                panel.error = None;
            }
            Task::none()
        }

        AppMessage::GroupSettingsLeaveCancelled => {
            if let Some(panel) = state.group_settings.as_mut() {
                panel.leave_confirm_open = false;
            }
            Task::none()
        }

        AppMessage::GroupSettingsLeaveConfirmed => {
            let Some(panel) = state.group_settings.as_mut() else {
                return Task::none();
            };
            if panel.submitting_leave {
                return Task::none();
            }
            let token = panel.open_token;
            let group_id = panel.group_id;
            panel.submitting_leave = true;
            panel.error = None;
            let bridge_arc = Arc::clone(bridge);
            Task::perform(
                async move { bridge_arc.leave_group(group_id).await },
                move |result| AppMessage::GroupSettingsLeaveResolved {
                    open_token: token,
                    result,
                },
            )
        }

        AppMessage::GroupSettingsLeaveResolved { open_token, result } => {
            let Some(panel) = state.group_settings.as_mut() else {
                return Task::none();
            };
            if panel.open_token != open_token {
                return Task::none();
            }
            panel.submitting_leave = false;
            match result {
                Ok(()) => {
                    let group_id = panel.group_id;
                    state.group_settings = None;
                    // 关闭该群的聊天面板（如当前正在查看）。
                    state.session_list.items.retain(|item| {
                        !(item.channel_id == group_id && item.channel_type == 2)
                    });
                    if let Some(chat) = state.active_chat.as_ref() {
                        if chat.channel_id == group_id && chat.channel_type == 2 {
                            state.active_chat = None;
                            state.active_read_channel_id = None;
                            state.route = Route::SessionList;
                        }
                    }
                }
                Err(err) => {
                    panel.leave_confirm_open = false;
                    panel.error = Some(format_ui_error(&err));
                }
            }
            Task::none()
        }

        AppMessage::GlobalMessageIngress {
            message_id,
            channel_id,
            channel_type,
            source,
        } => handle_global_message_ingress(
            state,
            bridge,
            message_id,
            channel_id,
            channel_type,
            source,
        ),

        AppMessage::GlobalMessageLoaded {
            message_id,
            channel_id,
            channel_type,
            source,
            message,
        } => {
            if let Some(ref msg) = message {
                eprintln!(
                    "[msg.loaded] id={} type={} is_own={} thumb={:?} local={:?} url={:?} file_id={:?} body_len={}",
                    msg.message_id, msg.message_type, msg.is_own,
                    msg.local_thumbnail_path, msg.media_local_path,
                    msg.media_url, msg.media_file_id, msg.body.len(),
                );
            }
            let media_tasks = message
                .as_ref()
                .map(|item| schedule_thumbnail_download_for_message(state, item, bridge))
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
            let core = handle_global_message_loaded(
                state,
                bridge,
                message_id,
                channel_id,
                channel_type,
                source,
                message,
            );
            let mut tasks = vec![core];
            tasks.extend(media_tasks);
            Task::batch(tasks)
        }

        AppMessage::GlobalMessageLoadFailed {
            message_id,
            channel_id,
            channel_type,
            source,
            error,
        } => {
            reporting::report_message_load_failed(
                source,
                message_id,
                channel_id,
                channel_type,
                &format_ui_error(&error),
            );
            Task::batch([
                schedule_session_list_refresh(state, bridge),
                schedule_total_unread_refresh(bridge),
            ])
        }

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
            let mut media_items: Option<Vec<MessageVm>> = None;
            if let Some(chat) = &mut state.active_chat {
                let applied = apply_timeline_patch(chat, patch);
                if applied {
                    chat.timeline.revision = revision;
                    chat.runtime_index.rebuild_from_items(&chat.timeline.items);
                    media_items = Some(chat.timeline.items.clone());
                }
            }
            if let Some(items) = media_items {
                let mut tasks = schedule_thumbnail_downloads_for_items(state, &items, bridge);
                tasks.extend(schedule_image_decodes(state));
                if should_refresh_unread {
                    tasks.push(schedule_total_unread_refresh(bridge));
                }
                if !tasks.is_empty() {
                    return Task::batch(tasks);
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
            let mut media_items: Option<Vec<MessageVm>> = None;
            if let Some(chat) = &mut state.active_chat {
                chat.timeline.is_loading_more = false;
                chat.timeline.oldest_server_message_id = page.oldest_server_message_id;
                chat.timeline.has_more_before = page.has_more_before;
                prepend_history_items(&mut chat.timeline.items, page.items);
                normalize_timeline_items(&mut chat.timeline.items);
                chat.runtime_index.rebuild_from_items(&chat.timeline.items);
                media_items = Some(chat.timeline.items.clone());
            }
            if let Some(items) = media_items {
                let mut tasks = schedule_thumbnail_downloads_for_items(state, &items, bridge);
                tasks.extend(schedule_image_decodes(state));
                if !tasks.is_empty() {
                    return Task::batch(tasks);
                }
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
            near_top,
        } => handle_viewport_changed(state, bridge, channel_id, channel_type, at_bottom, near_top),

        AppMessage::VoiceTogglePressed {
            message_id,
            created_at,
            local_path,
            file_id,
        } => handle_voice_toggle(state, bridge, message_id, created_at, local_path, file_id),

        AppMessage::VoicePlaybackFinished { message_id, result } => {
            if let Err(ref err) = result {
                tracing::warn!(message_id, error = ?err, "voice playback ended with error");
            }
            if state
                .voice_playback
                .as_ref()
                .map(|h| h.message_id == message_id)
                .unwrap_or(false)
            {
                state.voice_playback = None;
            }
            Task::none()
        }
    }
}

fn handle_voice_toggle(
    state: &mut AppState,
    bridge: &Arc<dyn SdkBridge>,
    message_id: u64,
    created_at: i64,
    local_path: Option<String>,
    file_id: Option<u64>,
) -> Task<AppMessage> {
    // 同一条再次点击 → 停止
    if let Some(existing) = state.voice_playback.as_ref() {
        if existing.message_id == message_id {
            let _ = existing.stop_tx.send(());
            state.voice_playback = None;
            return Task::none();
        }
        // 不同条：先停掉当前的，再起新的
        let _ = existing.stop_tx.send(());
        state.voice_playback = None;
    }

    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
    state.voice_playback = Some(crate::app::state::VoicePlaybackHandle {
        message_id,
        stop_tx,
    });

    let bridge = Arc::clone(bridge);
    let filename = Some(format!("{message_id}.m4a"));

    Task::perform(
        async move {
            // 1. 确保本地文件就位：已有则直接用，否则通过 SDK 下载。
            let path = match local_path {
                Some(p) if Path::new(&p).exists() => p,
                _ => {
                    let file_id = file_id.ok_or_else(|| {
                        crate::presentation::vm::UiError::Unknown(
                            "voice file_id missing".to_string(),
                        )
                    })?;
                    let url = bridge.get_file_url(file_id).await?;
                    bridge
                        .download_message_media_blocking(
                            message_id,
                            url,
                            "audio/mp4".to_string(),
                            filename,
                            created_at,
                        )
                        .await?
                }
            };

            // 2. 同步阻塞播放；主动停止由 stop_rx 决定
            tokio::task::spawn_blocking(move || play_voice_blocking(&path, stop_rx))
                .await
                .map_err(|e| {
                    crate::presentation::vm::UiError::Unknown(format!("voice join failed: {e}"))
                })?
        },
        move |result| AppMessage::VoicePlaybackFinished { message_id, result },
    )
}

/// 同步阻塞播放 m4a。以 rodio 解码 + Sink 播放，轮询 `stop_rx` 以允许主动停止。
/// 播放自然完成或收到停止信号后返回。
fn play_voice_blocking(
    path: &str,
    stop_rx: std::sync::mpsc::Receiver<()>,
) -> Result<(), crate::presentation::vm::UiError> {
    use std::fs::File;
    use std::time::Duration;

    let stream_handle = rodio::DeviceSinkBuilder::open_default_sink().map_err(|e| {
        crate::presentation::vm::UiError::Unknown(format!("voice output stream: {e}"))
    })?;
    let player = rodio::Player::connect_new(stream_handle.mixer());
    let file = File::open(path).map_err(|e| {
        crate::presentation::vm::UiError::Unknown(format!("voice open {path}: {e}"))
    })?;
    // 走 TryFrom<File>：自动补 byte_len + seekable=true。
    // m4a 的 moov 可能在文件尾部，非 seekable 路径会报 "not streamable"。
    let decoder = rodio::Decoder::try_from(file).map_err(|e| {
        crate::presentation::vm::UiError::Unknown(format!("voice decode: {e}"))
    })?;
    player.append(decoder);

    loop {
        if stop_rx.try_recv().is_ok() {
            player.stop();
            return Ok(());
        }
        if player.empty() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(80));
    }
}

fn add_friend_search_window_settings() -> window::Settings {
    window::Settings {
        size: Size::new(640.0, 840.0),
        min_size: Some(Size::new(520.0, 700.0)),
        resizable: true,
        decorations: true,
        level: window::Level::Normal,
        position: window::Position::Centered,
        ..window::Settings::default()
    }
}

fn logs_window_settings() -> window::Settings {
    window::Settings {
        size: Size::new(900.0, 620.0),
        min_size: Some(Size::new(680.0, 420.0)),
        resizable: true,
        decorations: true,
        level: window::Level::Normal,
        position: window::Position::Centered,
        ..window::Settings::default()
    }
}

fn image_viewer_window_settings() -> window::Settings {
    window::Settings {
        size: Size::new(900.0, 700.0),
        min_size: Some(Size::new(400.0, 300.0)),
        resizable: true,
        decorations: true,
        level: window::Level::Normal,
        position: window::Position::Centered,
        ..window::Settings::default()
    }
}

fn handle_conversation_selected(
    state: &mut AppState,
    bridge: &Arc<dyn SdkBridge>,
    channel_id: u64,
    channel_type: i32,
) -> Task<AppMessage> {
    state.overlay.settings_menu_open = false;

    if state.auth.user_id.is_none() {
        state.route = Route::Login;
        return Task::none();
    }

    let open_token = state.allocate_open_token();
    let resolved_title = resolve_chat_title(state, channel_id, channel_type);
    let peer_user_id = resolve_chat_peer_user_id(state, channel_id, channel_type);
    if let Some(user_id) = peer_user_id {
        match state.presences.get(&user_id) {
            Some(presence) => tracing::info!(
                "presence.local_cache_hit: channel_id={} channel_type={} open_token={} user_id={} is_online={} last_seen_at={} device_count={}",
                channel_id,
                channel_type,
                open_token,
                user_id,
                presence.is_online,
                presence.last_seen_at,
                presence.device_count
            ),
            None => tracing::info!(
                "presence.local_cache_miss: channel_id={} channel_type={} open_token={} user_id={}",
                channel_id,
                channel_type,
                open_token,
                user_id
            ),
        }
    } else {
        tracing::warn!(
            "presence.peer_user_id_missing: channel_id={} channel_type={} open_token={}",
            channel_id,
            channel_type,
            open_token
        );
    }
    tracing::info!(
        "presence.select_conversation: channel_id={} channel_type={} open_token={} peer_user_id={:?}",
        channel_id,
        channel_type,
        open_token,
        peer_user_id
    );
    state.route = Route::Chat;
    state.active_read_channel_id = Some(channel_id);
    state.active_chat = Some(ChatScreenState {
        channel_id,
        channel_type,
        peer_user_id,
        title: resolved_title,
        open_token,
        timeline: TimelineState::default(),
        runtime_index: RuntimeMessageIndex::default(),
        composer: ComposerState::default(),
        unread_marker: UnreadMarkerVm::default(),
        typing_hint: None,
        typing_user_id: None,

        peer_last_read_pts: None,
        attachment_menu: None,
        user_profile_panel: None,
        last_cursor_pos: None,
        delete_confirm: None,
        message_reactions: std::collections::HashMap::new(),
        reaction_picker_for: None,
    });
    clear_local_unread_for_channel(state, channel_id, channel_type);

    // open_timeline reads from local SQLite — must complete first, unblocked by network calls.
    // subscribe_channel and presence fetch both require network; they run after ConversationOpened.
    // Also fetch peer_read_pts for cold start display of "已读" status.
    let timeline_bridge = Arc::clone(bridge);
    let open_task = Task::perform(
        async move {
            let snapshot = timeline_bridge
                .open_timeline(channel_id, channel_type)
                .await?;
            let peer_read_pts = timeline_bridge
                .get_peer_read_pts(channel_id, channel_type)
                .await
                .unwrap_or(None);
            Ok((snapshot, peer_read_pts))
        },
        move |result: Result<_, UiError>| match result {
            Ok((snapshot, peer_read_pts)) => AppMessage::ConversationOpened {
                channel_id,
                channel_type,
                open_token,
                snapshot,
                peer_read_pts,
            },
            Err(error) => AppMessage::ConversationOpenFailed {
                channel_id,
                channel_type,
                open_token,
                error,
            },
        },
    );

    // 群聊额外拉取成员列表，用于 @ 提及候选。DM 跳过。
    if channel_type == 2 {
        let member_bridge = Arc::clone(bridge);
        let member_task = Task::perform(
            async move {
                member_bridge
                    .load_group_members(channel_id, channel_type)
                    .await
            },
            move |result: Result<_, UiError>| match result {
                Ok(members) => AppMessage::MentionMembersLoaded { channel_id, members },
                Err(error) => {
                    tracing::warn!(
                        "mention.load_group_members failed: channel_id={} channel_type={} error={}",
                        channel_id,
                        channel_type,
                        format_ui_error(&error)
                    );
                    AppMessage::MentionMembersLoaded {
                        channel_id,
                        members: Vec::new(),
                    }
                }
            },
        );
        return Task::batch([open_task, member_task]);
    }

    open_task
}

fn resolve_chat_title(state: &AppState, channel_id: u64, channel_type: i32) -> String {
    // When jumping from AddFriend detail, prefer the selected profile title so
    // we don't briefly flash a raw UID before session list refresh catches up.
    if matches!(state.route, Route::AddFriend) {
        if let Some(detail) = &state.add_friend.detail {
            let title = detail.title.trim();
            if !title.is_empty() {
                return title.to_string();
            }
        }
    }

    if let Some(item) = state
        .session_list
        .items
        .iter()
        .find(|item| item.channel_id == channel_id && item.channel_type == channel_type)
        .filter(|item| !item.title.trim().is_empty())
    {
        return item.title.clone();
    }

    if let Some(selection) = state.add_friend.selected_panel_item {
        return match selection {
            AddFriendSelectionVm::Friend(user_id) => state
                .add_friend
                .friends
                .iter()
                .find(|item| item.user_id == user_id)
                .map(|item| item.title.trim())
                .filter(|title| !title.is_empty())
                .map(ToString::to_string)
                .unwrap_or_else(|| "联系人".to_string()),
            AddFriendSelectionVm::Request(user_id) => state
                .add_friend
                .requests
                .iter()
                .find(|item| item.from_user_id == user_id)
                .map(|item| item.title.trim())
                .filter(|title| !title.is_empty())
                .map(ToString::to_string)
                .unwrap_or_else(|| "联系人".to_string()),
            AddFriendSelectionVm::Group(group_id) => state
                .add_friend
                .groups
                .iter()
                .find(|item| item.group_id == group_id)
                .map(|item| item.title.trim())
                .filter(|title| !title.is_empty())
                .map(ToString::to_string)
                .unwrap_or_else(|| "群聊".to_string()),
        };
    }

    if channel_type == 2 {
        "群聊".to_string()
    } else {
        "联系人".to_string()
    }
}

fn resolve_chat_peer_user_id(state: &AppState, channel_id: u64, channel_type: i32) -> Option<u64> {
    if let Some(item) = state
        .session_list
        .items
        .iter()
        .find(|item| item.channel_id == channel_id && item.channel_type == channel_type)
    {
        if item.peer_user_id.is_some() {
            return item.peer_user_id;
        }
    }
    None
}

fn infer_peer_user_id_from_timeline(
    items: &[MessageVm],
    current_user_id: Option<u64>,
) -> Option<u64> {
    items.iter().find_map(|item| {
        if item.from_uid == 0 {
            return None;
        }
        if Some(item.from_uid) == current_user_id {
            return None;
        }
        Some(item.from_uid)
    })
}

fn apply_presence_to_friend_items(state: &mut AppState) {
    for item in &mut state.add_friend.friends {
        item.is_online = state
            .presences
            .get(&item.user_id)
            .map(|presence| presence.is_online)
            .unwrap_or(false);
    }
}

fn schedule_friend_presence_refresh(
    state: &AppState,
    bridge: &Arc<dyn SdkBridge>,
) -> Task<AppMessage> {
    let user_ids = state
        .add_friend
        .friends
        .iter()
        .map(|item| item.user_id)
        .collect::<Vec<_>>();
    if user_ids.is_empty() {
        return Task::none();
    }

    let bridge = Arc::clone(bridge);
    Task::perform(
        async move { bridge.batch_get_presence(user_ids).await },
        |result| match result {
            Ok(items) => AppMessage::FriendPresencesLoaded { items },
            Err(error) => AppMessage::FriendPresencesLoadFailed { error },
        },
    )
}

fn schedule_session_peer_presence_refresh(
    state: &AppState,
    bridge: &Arc<dyn SdkBridge>,
) -> Task<AppMessage> {
    let mut seen = HashSet::new();
    let user_ids = state
        .session_list
        .items
        .iter()
        .take(30)
        .filter_map(|item| item.peer_user_id)
        .filter(|user_id| seen.insert(*user_id))
        .collect::<Vec<_>>();
    if user_ids.is_empty() {
        return Task::none();
    }

    let bridge = Arc::clone(bridge);
    Task::perform(
        async move { bridge.batch_get_presence(user_ids).await },
        |result| match result {
            Ok(items) => AppMessage::FriendPresencesLoaded { items },
            Err(error) => AppMessage::FriendPresencesLoadFailed { error },
        },
    )
}

fn schedule_presence_channel_subscriptions(
    state: &AppState,
    bridge: &Arc<dyn SdkBridge>,
) -> Task<AppMessage> {
    let direct_channels = state
        .session_list
        .items
        .iter()
        .filter(|item| item.peer_user_id.is_some())
        .map(|item| (item.channel_id, item.channel_type))
        .collect::<Vec<_>>();
    if direct_channels.is_empty() {
        return Task::none();
    }

    let tasks = direct_channels
        .into_iter()
        .map(|(channel_id, channel_type)| {
            let bridge = Arc::clone(bridge);
            Task::perform(
                async move { bridge.subscribe_channel(channel_id, channel_type).await },
                move |result| {
                    match result {
                        Ok(()) => tracing::info!(
                            "presence.schedule_subscribe.ok: channel_id={} channel_type={}",
                            channel_id,
                            channel_type
                        ),
                        Err(error) => tracing::warn!(
                            "presence.schedule_subscribe.failed: channel_id={} channel_type={} error={}",
                            channel_id,
                            channel_type,
                            format_ui_error(&error)
                        ),
                    }
                    AppMessage::Noop
                },
            )
        })
        .collect::<Vec<_>>();
    Task::batch(tasks)
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
    state.overlay.settings_menu_open = false;
    state.switch_account.add_account_login_mode = false;
    // Bump session_epoch so the SDK event subscription hash changes, forcing
    // Iced to tear down the old stream and create a fresh one with a new
    // broadcast::Receiver. This ensures events from the new user's session
    // are actually received.
    state.session_epoch = state.session_epoch.wrapping_add(1);
    state.presences.clear();
    state.active_read_channel_id = None;
    state.route = Route::SessionList;
}

fn apply_logout(state: &mut AppState) {
    state.overlay.settings_menu_open = false;
    state.switch_account.loading = false;
    state.switch_account.switching_uid = None;
    state.switch_account.error = None;
    state.switch_account.add_account_login_mode = false;
    state.active_chat = None;
    state.session_list.items.clear();
    state.session_list.load_error = None;
    state.session_list.total_unread_count = 0;
    state.session_list.is_loading = false;
    state.session_list.refresh_pending = false;
    state.add_friend.friends.clear();
    state.add_friend.groups.clear();
    state.add_friend.requests.clear();
    state.add_friend.selected_panel_item = None;
    state.add_friend.detail = None;
    state.add_friend.detail_error = None;
    state.add_friend.contacts_error = None;
    state.add_friend.search_results.clear();
    state.add_friend.feedback = None;
    state.presences.clear();
    state.active_read_channel_id = None;
    state.auth.is_submitting = false;
    state.auth.error = None;
    state.auth.password.clear();
    state.auth.user_id = None;
    state.auth.token = None;
    state.route = Route::Login;
}

fn resolve_switch_account_return_route(state: &AppState) -> Route {
    match state.switch_account.return_route {
        Route::SwitchAccount | Route::Login | Route::Splash => {
            if state.active_chat.is_some() {
                Route::Chat
            } else {
                Route::SessionList
            }
        }
        ref route => route.clone(),
    }
}

fn schedule_session_list_refresh(
    state: &mut AppState,
    bridge: &Arc<dyn SdkBridge>,
) -> Task<AppMessage> {
    if state.session_list.is_loading {
        // A load is already in-flight; coalesce by setting the pending flag.
        // When the current load completes (SessionListLoaded/Failed) it will
        // fire one more refresh to pick up any changes that arrived during the flight.
        state.session_list.refresh_pending = true;
        return Task::none();
    }
    state.session_list.is_loading = true;
    state.session_list.refresh_pending = false;
    let bridge = Arc::clone(bridge);
    Task::perform(
        async move { bridge.load_session_list().await },
        |result| match result {
            Ok(items) => AppMessage::SessionListLoaded { items },
            Err(error) => AppMessage::SessionListLoadFailed { error },
        },
    )
}

fn schedule_active_conversation_refresh(
    state: &AppState,
    bridge: &Arc<dyn SdkBridge>,
) -> Option<Task<AppMessage>> {
    let chat = state.active_chat.as_ref()?;
    let channel_id = chat.channel_id;
    let channel_type = chat.channel_type;
    let open_token = chat.open_token;
    let bridge = Arc::clone(bridge);
    Some(Task::perform(
        async move { bridge.open_timeline(channel_id, channel_type).await },
        move |result| match result {
            Ok(snapshot) => AppMessage::ActiveConversationRefreshed {
                channel_id,
                channel_type,
                open_token,
                snapshot,
            },
            Err(error) => AppMessage::ActiveConversationRefreshFailed {
                channel_id,
                channel_type,
                open_token,
                error,
            },
        },
    ))
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

fn schedule_channel_sync_repair(
    bridge: &Arc<dyn SdkBridge>,
    channel_id: u64,
    channel_type: i32,
) -> Task<AppMessage> {
    let bridge = Arc::clone(bridge);
    Task::perform(
        async move { bridge.sync_channel(channel_id, channel_type).await },
        move |result| match result {
            Ok(applied) => AppMessage::RepairChannelSyncSucceeded {
                channel_id,
                channel_type,
                applied,
            },
            Err(error) => AppMessage::RepairChannelSyncFailed {
                channel_id,
                channel_type,
                error,
            },
        },
    )
}

fn schedule_local_accounts_refresh(bridge: &Arc<dyn SdkBridge>) -> Task<AppMessage> {
    let bridge = Arc::clone(bridge);
    Task::perform(
        async move { bridge.list_local_accounts().await },
        |result| match result {
            Ok(accounts) => AppMessage::SwitchAccountListLoaded { accounts },
            Err(error) => AppMessage::SwitchAccountListLoadFailed { error },
        },
    )
}

fn schedule_active_username_refresh(bridge: &Arc<dyn SdkBridge>) -> Task<AppMessage> {
    let bridge = Arc::clone(bridge);
    Task::perform(
        async move { bridge.load_active_username().await },
        |result| match result {
            Ok(username) => AppMessage::ActiveUsernameLoaded { username },
            Err(error) => AppMessage::ActiveUsernameLoadFailed { error },
        },
    )
}

fn schedule_add_friend_refresh(bridge: &Arc<dyn SdkBridge>) -> Task<AppMessage> {
    let friends_bridge = Arc::clone(bridge);
    let groups_bridge = Arc::clone(bridge);
    let requests_bridge = Arc::clone(bridge);

    Task::batch([
        Task::perform(
            async move { friends_bridge.load_friend_list().await },
            |result| match result {
                Ok(items) => AppMessage::AddFriendFriendsLoaded { items },
                Err(error) => AppMessage::AddFriendFriendsLoadFailed { error },
            },
        ),
        Task::perform(
            async move { groups_bridge.load_group_list().await },
            |result| match result {
                Ok(items) => AppMessage::AddFriendGroupsLoaded { items },
                Err(error) => AppMessage::AddFriendGroupsLoadFailed { error },
            },
        ),
        Task::perform(
            async move { requests_bridge.load_friend_request_list().await },
            |result| match result {
                Ok(items) => AppMessage::AddFriendRequestsLoaded { items },
                Err(error) => AppMessage::AddFriendRequestsLoadFailed { error },
            },
        ),
    ])
}

fn schedule_add_friend_detail_load(
    bridge: &Arc<dyn SdkBridge>,
    item: AddFriendSelectionVm,
) -> Task<AppMessage> {
    let bridge = Arc::clone(bridge);
    Task::perform(
        async move { bridge.load_add_friend_detail(item).await },
        move |result| match result {
            Ok(detail) => AppMessage::AddFriendDetailLoaded { item, detail },
            Err(error) => AppMessage::AddFriendDetailLoadFailed { item, error },
        },
    )
}

fn sync_add_friend_flags(state: &mut AppState) {
    let friend_ids = state
        .add_friend
        .friends
        .iter()
        .map(|item| item.user_id)
        .collect::<HashSet<_>>();

    for user in &mut state.add_friend.search_results {
        if friend_ids.contains(&user.user_id) {
            user.is_friend = true;
        }
    }

    for request in &mut state.add_friend.requests {
        request.is_added = friend_ids.contains(&request.from_user_id);
    }
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

fn schedule_send_typing_task(
    bridge: &Arc<dyn SdkBridge>,
    channel_id: u64,
    channel_type: i32,
    is_typing: bool,
) -> Task<AppMessage> {
    let bridge = Arc::clone(bridge);
    Task::perform(
        async move {
            bridge
                .send_typing(channel_id, channel_type, is_typing)
                .await
        },
        move |result| match result {
            Ok(_) => AppMessage::TypingSendCompleted { is_typing },
            Err(error) => AppMessage::TypingSendFailed { is_typing, error },
        },
    )
}

fn schedule_typing_hint_expire_task(
    channel_id: u64,
    channel_type: i32,
    user_id: u64,
) -> Task<AppMessage> {
    Task::perform(
        async move {
            sleep(Duration::from_millis(TYPING_HINT_TTL_MILLIS)).await;
            AppMessage::TypingHintExpired {
                channel_id,
                channel_type,
                user_id,
            }
        },
        |message| message,
    )
}

fn handle_send_pressed(state: &mut AppState, bridge: &Arc<dyn SdkBridge>) -> Task<AppMessage> {
    let (body, channel_id, channel_type, open_token, reply_to_for_echo, mentioned_user_ids) =
        match state.active_chat.as_ref() {
            Some(chat_snapshot) => {
                let body = chat_snapshot.composer.draft.trim().to_string();
                if body.is_empty() {
                    return Task::none();
                }
                // 去重后收集现存 span 的 user_id；草稿若不含任何 span 就传 None。
                let mut seen = std::collections::HashSet::<u64>::new();
                let mut ids = Vec::<u64>::new();
                for span in &chat_snapshot.composer.mentions {
                    if seen.insert(span.user_id) {
                        ids.push(span.user_id);
                    }
                }
                let mentioned = if ids.is_empty() { None } else { Some(ids) };
                (
                    body,
                    chat_snapshot.channel_id,
                    chat_snapshot.channel_type,
                    chat_snapshot.open_token,
                    chat_snapshot
                        .composer
                        .pending_reply
                        .as_ref()
                        .map(|r| r.server_message_id),
                    mentioned,
                )
            }
            None => return Task::none(),
        };
    if channel_id == 0 || channel_type <= 0 {
        state.auth.error = Some(format!(
            "发送失败: 无效会话参数 channel_id={} channel_type={}",
            channel_id, channel_type
        ));
        return Task::none();
    }
    let client_txn_id = match bridge.generate_local_message_id() {
        Ok(id) => id,
        Err(error) => {
            warn!(
                "generate_local_message_id failed: {}",
                format_ui_error(&error)
            );
            state.auth.error = Some(format!("发送失败: {}", format_ui_error(&error)));
            return Task::none();
        }
    };
    let from_uid = state.auth.user_id.unwrap_or(0);
    let now = now_timestamp_millis();

    if let Some(chat) = &mut state.active_chat {
        let local_echo = MessageVm {
            key: TimelineItemKey::Local(client_txn_id),
            channel_id,
            channel_type,
            message_id: client_txn_id,
            server_message_id: None,
            client_txn_id: Some(client_txn_id),
            from_uid,
            body: body.clone(),
            message_type: TEXT_MESSAGE_TYPE,
            media_url: None,
            media_file_id: None,
            media_local_path: None,
            local_thumbnail_path: None,
            thumb_status: 0,
            media_file_size: None,
            voice_duration_secs: None,
            created_at: now,
            pts: None,
            send_state: Some(MessageSendStateVm::Sending),
            is_own: true,
            is_deleted: false,
            delivered: false,
            reply_to_server_message_id: reply_to_for_echo,
        };
        chat.timeline.items.push(local_echo);
        chat.runtime_index.bind(client_txn_id, client_txn_id);
        chat.composer.draft.clear();
        chat.composer.editor = iced::widget::text_editor::Content::new();
        chat.composer.emoji_picker_open = false;
        chat.composer.quick_phrase_open = false;
        chat.composer.typing_active = false;
        chat.composer.pending_reply = None;
    }
    touch_session_preview(state, channel_id, channel_type, &body, now);

    // 顺序执行：先通知对方"输入结束"，再发送消息
    // 确保对方先看到"正在输入"消失，再收到新消息
    let send_bridge = Arc::clone(bridge);
    let typing_bridge = Arc::clone(bridge);
    let send_task = Task::perform(
        async move {
            // 1. 先发送 typing=false，通知对方输入结束
            if let Err(e) = typing_bridge
                .send_typing(channel_id, channel_type, false)
                .await
            {
                warn!(
                    "send_typing(false) before send_message failed: {}",
                    format_ui_error(&e)
                );
            }
            // 2. 再发送实际消息
            send_bridge
                .send_text_message(
                    channel_id,
                    channel_type,
                    client_txn_id,
                    body,
                    reply_to_for_echo,
                    mentioned_user_ids,
                )
                .await
        },
        move |result| match result {
            Ok(message_id) => AppMessage::TimelineUpdatedIngress {
                channel_id,
                channel_type,
                open_token,
                message_id,
            },
            Err(error) => {
                warn!(
                    "send_text_message failed: channel_id={} channel_type={} client_txn_id={} error={}",
                    channel_id, channel_type, client_txn_id, format_ui_error(&error)
                );
                AppMessage::TimelinePatched {
                    channel_id,
                    channel_type,
                    open_token,
                    revision: events::allocate_patch_revision(),
                    patch: TimelinePatchVm::RemoveMessage {
                        key: TimelineItemKey::Local(client_txn_id),
                    },
                }
            }
        },
    );
    // typing=false 已在 send_task 内部顺序发送，不再需要额外 batch
    send_task
}

fn attachment_type_body_and_preview(path: &Path) -> (i32, String, String) {
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("file")
        .to_string();
    match ext.as_str() {
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "heic" => (
            IMAGE_MESSAGE_TYPE,
            "[图片]".to_string(),
            "[图片]".to_string(),
        ),
        "mp4" | "mov" | "mkv" | "avi" | "webm" => {
            (VIDEO_MESSAGE_TYPE, filename, "[视频]".to_string())
        }
        _ => (FILE_MESSAGE_TYPE, filename, "[文件]".to_string()),
    }
}

fn is_image_file_path(path: &str) -> bool {
    let ext = Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    matches!(
        ext.as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "heic"
    )
}

fn handle_send_attachment_path(
    state: &mut AppState,
    bridge: &Arc<dyn SdkBridge>,
    file_path: String,
) -> Task<AppMessage> {
    let (channel_id, channel_type, open_token) = match state.active_chat.as_ref() {
        Some(chat) => (chat.channel_id, chat.channel_type, chat.open_token),
        None => return Task::none(),
    };
    if channel_id == 0 || channel_type <= 0 {
        state.auth.error = Some(format!(
            "发送失败: 无效会话参数 channel_id={} channel_type={}",
            channel_id, channel_type
        ));
        return Task::none();
    }

    let client_txn_id = match bridge.generate_local_message_id() {
        Ok(id) => id,
        Err(error) => {
            warn!(
                "generate_local_message_id failed for attachment: {}",
                format_ui_error(&error)
            );
            state.auth.error = Some(format!("发送失败: {}", format_ui_error(&error)));
            return Task::none();
        }
    };

    let path = Path::new(&file_path);
    let (message_type, body, preview) = attachment_type_body_and_preview(path);
    let local_file_size = fs::metadata(path).ok().map(|m| m.len());
    let from_uid = state.auth.user_id.unwrap_or(0);
    let now = now_timestamp_millis();

    if let Some(chat) = &mut state.active_chat {
        let local_echo = MessageVm {
            key: TimelineItemKey::Local(client_txn_id),
            channel_id,
            channel_type,
            message_id: client_txn_id,
            server_message_id: None,
            client_txn_id: Some(client_txn_id),
            from_uid,
            body,
            message_type,
            media_url: None,
            media_file_id: None,
            media_local_path: if message_type == IMAGE_MESSAGE_TYPE {
                Some(file_path.clone())
            } else {
                None
            },
            local_thumbnail_path: None,
            thumb_status: 0,
            media_file_size: local_file_size,
            voice_duration_secs: None,
            created_at: now,
            pts: None,
            send_state: Some(MessageSendStateVm::Sending),
            is_own: true,
            is_deleted: false,
            delivered: false,
            reply_to_server_message_id: None,
        };
        chat.timeline.items.push(local_echo);
        chat.runtime_index.bind(client_txn_id, client_txn_id);
        chat.composer.emoji_picker_open = false;
        chat.composer.quick_phrase_open = false;
        chat.composer.typing_active = false;
    }
    touch_session_preview(state, channel_id, channel_type, &preview, now);

    // 顺序执行：先通知对方"输入结束"，再发送附件消息
    let send_bridge = Arc::clone(bridge);
    let typing_bridge = Arc::clone(bridge);
    let send_task = Task::perform(
        async move {
            // 1. 先发送 typing=false，通知对方输入结束
            if let Err(e) = typing_bridge
                .send_typing(channel_id, channel_type, false)
                .await
            {
                warn!(
                    "send_typing(false) before send_attachment failed: {}",
                    format_ui_error(&e)
                );
            }
            // 2. 再发送附件消息
            send_bridge
                .send_attachment_message(channel_id, channel_type, client_txn_id, file_path)
                .await
        },
        move |result| match result {
            Ok(message_id) => AppMessage::TimelineUpdatedIngress {
                channel_id,
                channel_type,
                open_token,
                message_id,
            },
            Err(error) => {
                warn!(
                        "send_attachment_message failed: channel_id={} channel_type={} client_txn_id={} error={}",
                        channel_id, channel_type, client_txn_id, format_ui_error(&error)
                    );
                AppMessage::TimelinePatched {
                    channel_id,
                    channel_type,
                    open_token,
                    revision: events::allocate_patch_revision(),
                    patch: TimelinePatchVm::RemoveMessage {
                        key: TimelineItemKey::Local(client_txn_id),
                    },
                }
            }
        },
    );
    // typing=false 已在 send_task 内部顺序发送
    send_task
}

fn touch_session_preview(
    state: &mut AppState,
    channel_id: u64,
    channel_type: i32,
    preview: &str,
    timestamp_ms: i64,
) {
    if let Some(item) = state
        .session_list
        .items
        .iter_mut()
        .find(|entry| entry.channel_id == channel_id && entry.channel_type == channel_type)
    {
        item.subtitle = preview.to_string();
        item.last_msg_timestamp = timestamp_ms;
    }
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
            Err(error) => {
                warn!(
                    "retry_send failed: channel_id={} channel_type={} client_txn_id={} error={}",
                    channel_id,
                    channel_type,
                    client_txn_id,
                    format_ui_error(&error)
                );
                AppMessage::TimelinePatched {
                    channel_id,
                    channel_type,
                    open_token,
                    revision: events::allocate_patch_revision(),
                    patch: TimelinePatchVm::UpdateSendState {
                        client_txn_id,
                        send_state: MessageSendStateVm::FailedRetryable { reason: error },
                    },
                }
            }
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
                if remote.channel_id != channel_id || remote.channel_type != channel_type {
                    warn!(
                        "timeline ingress channel mismatch: msg={} got={}/{} expected={}/{}",
                        message_id,
                        remote.channel_id,
                        remote.channel_type,
                        channel_id,
                        channel_type
                    );
                    return AppMessage::RefreshSessionList;
                }
                let resolved_client_txn_id = replacement_client_txn_id.or(remote.client_txn_id);
                if remote.server_message_id.is_none() {
                    if let Some(client_txn_id) = resolved_client_txn_id {
                        if let Some(send_state) = remote.send_state.clone() {
                            if !matches!(send_state, MessageSendStateVm::Queued) {
                                return AppMessage::TimelinePatched {
                                    channel_id,
                                    channel_type,
                                    open_token,
                                    revision: events::allocate_patch_revision(),
                                    patch: TimelinePatchVm::UpdateSendState {
                                        client_txn_id,
                                        send_state,
                                    },
                                };
                            }
                        }
                    }
                    return AppMessage::Noop;
                }
                let (patch, patch_kind) = match resolved_client_txn_id {
                    Some(client_txn_id) => (
                        TimelinePatchVm::ReplaceLocalEcho {
                            client_txn_id,
                            remote,
                        },
                        TimelinePatchKind::ReplaceLocalEcho,
                    ),
                    None => (
                        TimelinePatchVm::UpsertRemote { remote },
                        TimelinePatchKind::UpsertRemote,
                    ),
                };
                reporting::report_timeline_patch(patch_kind, channel_id, channel_type);

                AppMessage::TimelinePatched {
                    channel_id,
                    channel_type,
                    open_token,
                    revision: events::allocate_patch_revision(),
                    patch,
                }
            }
            Ok(None) => AppMessage::Noop,
            Err(error) => {
                warn!(
                    "timeline ingress load_message_vm failed: message_id={} error={}",
                    message_id,
                    format_ui_error(&error)
                );
                AppMessage::Noop
            }
        },
    )
}

fn handle_global_message_ingress(
    _state: &mut AppState,
    bridge: &Arc<dyn SdkBridge>,
    message_id: u64,
    channel_id: Option<u64>,
    channel_type: Option<i32>,
    source: MessageIngressSource,
) -> Task<AppMessage> {
    reporting::report_message_ingress(source, message_id, channel_id, channel_type);

    let bridge = Arc::clone(bridge);
    Task::perform(
        async move { bridge.load_message_vm(message_id).await },
        move |result| match result {
            Ok(message) => AppMessage::GlobalMessageLoaded {
                message_id,
                channel_id,
                channel_type,
                source,
                message,
            },
            Err(error) => AppMessage::GlobalMessageLoadFailed {
                message_id,
                channel_id,
                channel_type,
                source,
                error,
            },
        },
    )
}

fn handle_global_message_loaded(
    state: &mut AppState,
    bridge: &Arc<dyn SdkBridge>,
    message_id: u64,
    channel_id: Option<u64>,
    channel_type: Option<i32>,
    source: MessageIngressSource,
    message: Option<MessageVm>,
) -> Task<AppMessage> {
    let Some(message) = message else {
        reporting::report_message_missing(source, message_id, channel_id, channel_type);
        return Task::batch([
            schedule_session_list_refresh(state, bridge),
            schedule_total_unread_refresh(bridge),
        ]);
    };

    reporting::report_message_loaded(source, &message);
    maybe_play_message_notification_sound(state, source, &message);
    apply_global_message_to_active_chat(state, message);
    Task::batch([
        schedule_session_list_refresh(state, bridge),
        schedule_total_unread_refresh(bridge),
    ])
}

fn apply_global_message_to_active_chat(state: &mut AppState, remote: MessageVm) {
    let Some(chat) = &mut state.active_chat else {
        return;
    };
    if chat.channel_id != remote.channel_id || chat.channel_type != remote.channel_type {
        return;
    }

    let incoming_server_message_id = remote.server_message_id;
    let incoming_is_own = remote.is_own;
    let existed_before = incoming_server_message_id
        .map(|server_message_id| {
            find_item_index_by_server_message_id(&chat.timeline.items, server_message_id).is_some()
        })
        .unwrap_or(false);

    let resolved_client_txn_id = chat
        .runtime_index
        .client_txn_id_for_message(remote.message_id)
        .or(remote.client_txn_id);

    let applied = if remote.server_message_id.is_none() {
        if let Some(client_txn_id) = resolved_client_txn_id {
            if let Some(send_state) = remote.send_state.clone() {
                if !matches!(send_state, MessageSendStateVm::Queued) {
                    apply_update_send_state_patch(
                        &mut chat.timeline.items,
                        client_txn_id,
                        send_state,
                    )
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        }
    } else {
        let (patch, patch_kind) = match resolved_client_txn_id {
            Some(client_txn_id) => (
                TimelinePatchVm::ReplaceLocalEcho {
                    client_txn_id,
                    remote,
                },
                TimelinePatchKind::ReplaceLocalEcho,
            ),
            None => (
                TimelinePatchVm::UpsertRemote { remote },
                TimelinePatchKind::UpsertRemote,
            ),
        };
        let applied = apply_timeline_patch(chat, patch);
        if applied {
            reporting::report_timeline_patch(patch_kind, chat.channel_id, chat.channel_type);
        }
        applied
    };

    if applied {
        normalize_timeline_items(&mut chat.timeline.items);
        chat.timeline.revision = events::allocate_patch_revision();
        chat.runtime_index.rebuild_from_items(&chat.timeline.items);
        if !chat.timeline.at_bottom && !incoming_is_own && !existed_before {
            if let Some(server_message_id) = incoming_server_message_id {
                if chat.unread_marker.first_unread_key.is_none() {
                    chat.unread_marker.first_unread_key =
                        Some(TimelineItemKey::Remote { server_message_id });
                }
                chat.unread_marker.unread_count = chat.unread_marker.unread_count.saturating_add(1);
                chat.unread_marker.has_unread_below_viewport = true;
            }
        }
    }
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
    near_top: bool,
) -> Task<AppMessage> {
    let Some(chat) = &mut state.active_chat else {
        return Task::none();
    };
    if chat.channel_id != channel_id || chat.channel_type != channel_type {
        return Task::none();
    }

    chat.timeline.at_bottom = at_bottom;

    if near_top {
        return handle_load_older_triggered(state, bridge, channel_id, channel_type);
    }

    if !at_bottom {
        return Task::none();
    }

    let last_read_pts = latest_read_pts(&chat.timeline.items).unwrap_or(0);

    // Read Gate v1: Strictly bind unread clearing to the active reading context
    if state.active_read_channel_id == Some(channel_id) {
        clear_local_unread_for_channel(state, channel_id, channel_type);
        return maybe_auto_mark_read(state, bridge, channel_id, channel_type, last_read_pts);
    }
    Task::none()
}

fn clear_local_unread_for_channel(state: &mut AppState, channel_id: u64, channel_type: i32) {
    let mut cleared = 0_u32;
    if let Some(item) = state
        .session_list
        .items
        .iter_mut()
        .find(|entry| entry.channel_id == channel_id && entry.channel_type == channel_type)
    {
        cleared = item.unread_count;
        item.unread_count = 0;
    }
    if cleared > 0 {
        state.session_list.total_unread_count = state
            .session_list
            .total_unread_count
            .saturating_sub(cleared);
    }

    if let Some(chat) = &mut state.active_chat {
        if chat.channel_id == channel_id && chat.channel_type == channel_type {
            chat.unread_marker.first_unread_key = None;
            chat.unread_marker.unread_count = 0;
            chat.unread_marker.has_unread_below_viewport = false;
        }
    }
}

/// Read Gate v1 API

/// 进入会话阅读态：显式激活自动已读上下文
fn enter_reading_conversation(state: &mut AppState, channel_id: u64) {
    state.active_read_channel_id = Some(channel_id);
}

/// 离开会话阅读态：显式失活自动已读上下文
fn leave_reading_conversation(state: &mut AppState) {
    state.active_read_channel_id = None;
}

/// 自动已读统一门禁：只有当前激活的会话才允许推进 read cursor
/// 如果 channel_id 不匹配 active_read_channel_id，直接静默拦截。
fn maybe_auto_mark_read(
    state: &AppState,
    bridge: &Arc<dyn SdkBridge>,
    channel_id: u64,
    channel_type: i32,
    last_read_pts: u64,
) -> Task<AppMessage> {
    if state.active_read_channel_id != Some(channel_id) {
        return Task::none();
    }
    schedule_mark_read_task(bridge, channel_id, channel_type, last_read_pts)
}

fn schedule_mark_read_task(
    bridge: &Arc<dyn SdkBridge>,
    channel_id: u64,
    channel_type: i32,
    last_read_pts: u64,
) -> Task<AppMessage> {
    let bridge = Arc::clone(bridge);
    Task::perform(
        async move {
            bridge
                .mark_read(channel_id, channel_type, last_read_pts)
                .await
        },
        move |result| match result {
            Ok(()) => {
                tracing::info!(
                    "mark_read ok: channel_id={} channel_type={} last_read_pts={}",
                    channel_id,
                    channel_type,
                    last_read_pts
                );
                AppMessage::RefreshTotalUnreadCount
            }
            Err(error) => {
                warn!(
                    "mark_read failed: channel_id={} channel_type={} last_read_pts={} error={}",
                    channel_id,
                    channel_type,
                    last_read_pts,
                    format_ui_error(&error)
                );
                AppMessage::Noop
            }
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
    let applied = match patch {
        TimelinePatchVm::ReplaceLocalEcho {
            client_txn_id,
            mut remote,
        } => {
            let Some(server_message_id) = remote.server_message_id else {
                warn!("ignore ReplaceLocalEcho without server_message_id");
                return false;
            };
            let preserved_local_path =
                find_item_index_by_client_txn(&chat.timeline.items, client_txn_id)
                    .and_then(|index| chat.timeline.items[index].media_local_path.clone());
            if remote.media_local_path.is_none() {
                remote.media_local_path = preserved_local_path;
            }
            remote.client_txn_id = Some(client_txn_id);
            remote.key = TimelineItemKey::Remote { server_message_id };

            // 消息强收敛：收到远端消息时，如果发送者正在输入，则清除其气泡
            if chat.typing_user_id == Some(remote.from_uid) {
                chat.typing_hint = None;
                chat.typing_user_id = None;
            }

            if let Some(index) = find_item_index_by_client_txn(&chat.timeline.items, client_txn_id)
            {
                preserve_monotonic_flags(&mut remote, &chat.timeline.items[index]);
                chat.timeline.items[index] = remote;
                dedup_remote_key(&mut chat.timeline.items, server_message_id, index);
                true
            } else if let Some(index) =
                find_item_index_by_server_message_id(&chat.timeline.items, server_message_id)
            {
                preserve_monotonic_flags(&mut remote, &chat.timeline.items[index]);
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

            // 消息强收敛：收到远端消息时，如果发送者正在输入，则清除其气泡
            if chat.typing_user_id == Some(remote.from_uid) {
                chat.typing_hint = None;
                chat.typing_user_id = None;
            }

            if let Some(index) =
                find_item_index_by_server_message_id(&chat.timeline.items, server_message_id)
            {
                preserve_monotonic_flags(&mut remote, &chat.timeline.items[index]);
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
    };

    if applied {
        normalize_timeline_items(&mut chat.timeline.items);
    }
    applied
}

fn normalize_timeline_items(items: &mut [MessageVm]) {
    items.sort_by_key(timeline_order_key);
}

// delivered 是 0→1 单调 receipt 语义：MessageDelivered 事件和 DB CAS 都只从 false 升到 true。
// Timeline upsert 做整项 replace，若 remote 来自早一拍的 DB 读，可能携带 delivered=false
// 把已升级的内存态覆盖回去。这里用 OR-merge 阻断单调状态回退。
fn preserve_monotonic_flags(remote: &mut MessageVm, current: &MessageVm) {
    remote.delivered = remote.delivered || current.delivered;
}

fn timeline_order_key(item: &MessageVm) -> u64 {
    // Use DB row id as the canonical order key:
    // smaller id first, larger id last.
    item.message_id
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
            | (Queued, Sent)
            | (Sending, Sent)
            | (Sending, FailedRetryable { .. })
            | (Sending, FailedPermanent { .. })
            | (Sending, Retrying)
            | (FailedRetryable { .. }, Sending)
            | (FailedRetryable { .. }, Retrying)
            | (FailedRetryable { .. }, Sent)
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

fn is_already_revoked_error(error: &crate::presentation::vm::UiError) -> bool {
    format_ui_error(error).contains("已被撤回")
}

fn is_revoke_timeout_error(error: &crate::presentation::vm::UiError) -> bool {
    let text = format_ui_error(error);
    text.contains("撤回时限")
        || text.contains("超过")
            && (text.contains("2分钟") || text.contains("120") || text.contains("时限"))
}

enum ClipboardPastePayload {
    AttachmentPath(String),
    PlainText(String),
}

fn copy_text_to_clipboard(value: &str) -> Result<(), crate::presentation::vm::UiError> {
    let mut clipboard = arboard::Clipboard::new().map_err(|error| {
        crate::presentation::vm::UiError::Unknown(format!("初始化剪贴板失败: {error}"))
    })?;
    clipboard.set_text(value.to_string()).map_err(|error| {
        crate::presentation::vm::UiError::Unknown(format!("写入剪贴板失败: {error}"))
    })
}

fn handle_composer_paste_pressed(
    state: &mut AppState,
    bridge: &Arc<dyn SdkBridge>,
) -> Task<AppMessage> {
    if state.active_chat.is_none() {
        return Task::none();
    }

    let payload = match read_clipboard_payload(state.auth.user_id.unwrap_or(0)) {
        Ok(payload) => payload,
        Err(error) => {
            state.auth.error = Some(format!("读取剪贴板失败: {}", format_ui_error(&error)));
            return Task::none();
        }
    };

    match payload {
        Some(ClipboardPastePayload::AttachmentPath(path)) => {
            Task::done(AppMessage::ComposerAttachmentPicked { path: Some(path) })
        }
        Some(ClipboardPastePayload::PlainText(text)) => {
            if text.trim().is_empty() {
                return Task::none();
            }
            if let Some(chat) = &mut state.active_chat {
                let was_typing = chat.composer.typing_active;
                chat.composer
                    .editor
                    .perform(iced::widget::text_editor::Action::Edit(
                        iced::widget::text_editor::Edit::Paste(Arc::new(text)),
                    ));
                chat.composer.draft = chat.composer.editor.text();
                let is_typing = !chat.composer.draft.trim().is_empty();
                chat.composer.typing_active = is_typing;
                if was_typing != is_typing {
                    return schedule_send_typing_task(
                        bridge,
                        chat.channel_id,
                        chat.channel_type,
                        is_typing,
                    );
                }
            }
            Task::none()
        }
        None => Task::none(),
    }
}

fn read_clipboard_payload(
    user_id: u64,
) -> Result<Option<ClipboardPastePayload>, crate::presentation::vm::UiError> {
    let mut clipboard = arboard::Clipboard::new().map_err(|error| {
        crate::presentation::vm::UiError::Unknown(format!("初始化剪贴板失败: {error}"))
    })?;

    if let Ok(image) = clipboard.get_image() {
        let path = save_clipboard_image(user_id, image)?;
        return Ok(Some(ClipboardPastePayload::AttachmentPath(path)));
    }

    if let Ok(text) = clipboard.get_text() {
        if let Some(path) = parse_clipboard_file_path(&text) {
            return Ok(Some(ClipboardPastePayload::AttachmentPath(path)));
        }
        return Ok(Some(ClipboardPastePayload::PlainText(text)));
    }

    Ok(None)
}

fn parse_clipboard_file_path(raw: &str) -> Option<String> {
    let first = raw.lines().next()?.trim();
    if first.is_empty() {
        return None;
    }

    let trimmed = first
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches('<')
        .trim_matches('>');
    let normalized = if let Some(path) = trimmed.strip_prefix("file://") {
        path.replace("%20", " ")
    } else {
        trimmed.to_string()
    };

    let path = Path::new(&normalized);
    if path.exists() && path.is_file() {
        Some(normalized)
    } else {
        None
    }
}

fn save_clipboard_image(
    user_id: u64,
    image_data: arboard::ImageData<'_>,
) -> Result<String, crate::presentation::vm::UiError> {
    let width = image_data.width as u32;
    let height = image_data.height as u32;
    let bytes = image_data.bytes.into_owned();
    let image = image::RgbaImage::from_raw(width, height, bytes).ok_or_else(|| {
        crate::presentation::vm::UiError::Unknown("剪贴板图片解码失败".to_string())
    })?;

    let base = Path::new("/tmp")
        .join("privchat-iced")
        .join("clipboard")
        .join(user_id.to_string());
    fs::create_dir_all(&base).map_err(|error| {
        crate::presentation::vm::UiError::Unknown(format!("创建剪贴板缓存目录失败: {error}"))
    })?;
    let file_name = format!("paste-{}.png", now_timestamp_millis());
    let target = base.join(file_name);
    image
        .save_with_format(&target, image::ImageFormat::Png)
        .map_err(|error| {
            crate::presentation::vm::UiError::Unknown(format!("写入剪贴板图片失败: {error}"))
        })?;
    Ok(target.to_string_lossy().to_string())
}

fn schedule_thumbnail_downloads_for_items(
    state: &mut AppState,
    items: &[MessageVm],
    bridge: &Arc<dyn SdkBridge>,
) -> Vec<Task<AppMessage>> {
    const DOWNLOAD_WINDOW: usize = 12;
    let start = items.len().saturating_sub(DOWNLOAD_WINDOW);
    items[start..]
        .iter()
        .filter_map(|item| schedule_thumbnail_download_for_message(state, item, bridge))
        .collect()
}

/// Scan timeline items for image messages that need async decoding.
/// Returns Tasks that decode images in background and deliver Handle via ImageDecoded.
fn schedule_image_decodes(state: &mut AppState) -> Vec<Task<AppMessage>> {
    const DECODE_WINDOW: usize = 12;
    let items: Vec<(u64, String)> = state
        .active_chat
        .as_ref()
        .map(|chat| {
            let start = chat.timeline.items.len().saturating_sub(DECODE_WINDOW);
            chat.timeline.items[start..]
                .iter()
                .filter(|item| {
                    matches!(item.message_type, IMAGE_MESSAGE_TYPE | VIDEO_MESSAGE_TYPE)
                        && !item.is_deleted
                })
                .filter(|item| {
                    !state.image_cache.contains_key(&item.message_id)
                        && !state.image_decode_pending.contains(&item.message_id)
                })
                .filter_map(|item| {
                    let path = item
                        .local_thumbnail_path
                        .as_deref()
                        .or(item.media_local_path.as_deref())?;
                    if Path::new(path).exists() {
                        Some((item.message_id, path.to_string()))
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    items
        .into_iter()
        .map(|(message_id, path)| {
            state.image_decode_pending.insert(message_id);
            Task::perform(
                async move { decode_image_to_rgba(path).await },
                move |result| match result {
                    Some(handle) => AppMessage::ImageDecoded { message_id, handle },
                    None => AppMessage::ImageDecodeFailed { message_id },
                },
            )
        })
        .collect()
}

/// Decode an image file into an iced Handle (RGBA) in a blocking task.
/// Resizes to fit within 440x320 for display.
async fn decode_image_to_rgba(path: String) -> Option<iced::widget::image::Handle> {
    tokio::task::spawn_blocking(move || {
        let bytes = std::fs::read(&path).ok()?;
        let img = ::image::load_from_memory(&bytes).ok()?;
        let resized = img.resize(440, 320, ::image::imageops::FilterType::Triangle);
        let rgba = resized.to_rgba8();
        let (w, h) = rgba.dimensions();
        Some(iced::widget::image::Handle::from_rgba(w, h, rgba.into_raw()))
    })
    .await
    .ok()?
}

fn schedule_thumbnail_download_for_message(
    state: &mut AppState,
    item: &MessageVm,
    bridge: &Arc<dyn SdkBridge>,
) -> Option<Task<AppMessage>> {
    if item.message_type != IMAGE_MESSAGE_TYPE {
        return None;
    }
    if item.thumb_status == 3 {
        return None;
    }
    if let Some(thumb_path) = item.local_thumbnail_path.as_ref() {
        if Path::new(thumb_path).exists() {
            return None;
        }
    }
    if let Some(local_path) = item.media_local_path.as_ref() {
        if Path::new(local_path).exists() && !is_thumbnail_local_path(local_path) {
            return None;
        }
    }
    let user_id = state.auth.user_id?;
    if !state.media_downloads_inflight.insert(item.message_id) {
        return None;
    }

    let message_id = item.message_id;
    let created_at = item.created_at;
    // 保存到 canonical thumb.webp 路径，与 SDK 规范一致
    let yyyymm = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(created_at)
        .map(|dt| dt.format("%Y%m").to_string())
        .unwrap_or_else(|| chrono::Utc::now().format("%Y%m").to_string());
    let target_path = media_data_root()
        .join("users")
        .join(user_id.to_string())
        .join("files")
        .join(yyyymm)
        .join(message_id.to_string())
        .join("thumb.webp")
        .to_string_lossy()
        .to_string();

    if let Some(file_id) = item.media_file_id {
        let bridge = Arc::clone(bridge);
        Some(Task::perform(
            async move {
                let url = bridge.get_file_url(file_id).await?;
                download_image_thumbnail(message_id, url, target_path).await
            },
            move |result| match result {
                Ok(local_path) => AppMessage::MediaThumbnailDownloaded {
                    message_id,
                    local_path,
                },
                Err(_) => AppMessage::MediaThumbnailDownloadFailed {
                    message_id,
                    error: result.unwrap_err(),
                },
            },
        ))
    } else if let Some(url) = item.media_url.clone() {
        Some(Task::perform(
            async move { download_image_thumbnail(message_id, url, target_path).await },
            move |result| match result {
                Ok(local_path) => AppMessage::MediaThumbnailDownloaded {
                    message_id,
                    local_path,
                },
                Err(_) => AppMessage::MediaThumbnailDownloadFailed {
                    message_id,
                    error: result.unwrap_err(),
                },
            },
        ))
    } else {
        state.media_downloads_inflight.remove(&message_id);
        None
    }
}

/// 通过 magic bytes 检测图片真实格式
fn detect_image_extension(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\x89PNG") {
        "png"
    } else if bytes.starts_with(b"\xFF\xD8\xFF") {
        "jpg"
    } else if bytes.starts_with(b"GIF8") {
        "gif"
    } else if bytes.starts_with(b"RIFF") && bytes.len() > 12 && &bytes[8..12] == b"WEBP" {
        "webp"
    } else if bytes.starts_with(b"BM") {
        "bmp"
    } else {
        "png" // 默认 PNG
    }
}

fn infer_file_extension(url: &str) -> &'static str {
    let lower = url.to_ascii_lowercase();
    for ext in ["jpg", "jpeg", "png", "gif", "webp", "bmp", "heic"] {
        let needle = format!(".{ext}");
        if lower.contains(&needle) {
            return ext;
        }
    }
    "jpg"
}

async fn download_image_thumbnail(
    _message_id: u64,
    url: String,
    target_path: String,
) -> Result<String, crate::presentation::vm::UiError> {
    let response = match reqwest::get(&url).await {
        Ok(resp) => resp,
        Err(error) => {
            return Err(crate::presentation::vm::UiError::Unknown(format!(
                "error sending request for url ({url}): {error}"
            )));
        }
    };
    let response = match response.error_for_status() {
        Ok(resp) => resp,
        Err(error) => {
            return Err(crate::presentation::vm::UiError::Unknown(format!(
                "download thumbnail bad status for url ({url}): {error}"
            )));
        }
    };
    let bytes = match response.bytes().await {
        Ok(bytes) => bytes,
        Err(error) => {
            return Err(crate::presentation::vm::UiError::Unknown(format!(
                "read thumbnail body failed for url ({url}): {error}"
            )));
        }
    };

    let mut file_path = PathBuf::from(target_path);
    // 检测真实图片格式，修正扩展名（避免 PNG 存为 .webp 等导致 iced 无法加载）
    let real_ext = detect_image_extension(&bytes);
    file_path.set_extension(real_ext);

    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            crate::presentation::vm::UiError::Unknown(format!(
                "create thumbnail cache failed: {error}"
            ))
        })?;
    }
    fs::write(&file_path, &bytes).map_err(|error| {
        crate::presentation::vm::UiError::Unknown(format!("write thumbnail failed: {error}"))
    })?;
    Ok(file_path.to_string_lossy().to_string())
}

fn media_thumbnail_cache_path(
    user_id: u64,
    created_at_ms: i64,
    message_id: u64,
    media_url: &str,
) -> PathBuf {
    let yyyymm = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(created_at_ms)
        .map(|dt| dt.format("%Y%m").to_string())
        .unwrap_or_else(|| chrono::Utc::now().format("%Y%m").to_string());
    let extension = infer_file_extension(media_url);
    media_data_root()
        .join("users")
        .join(user_id.to_string())
        .join("files")
        .join(yyyymm)
        .join(message_id.to_string())
        .join(format!("thumb.{extension}"))
}

fn is_thumbnail_local_path(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|v| v.to_str())
        .map(|name| name.to_ascii_lowercase().starts_with("thumb."))
        .unwrap_or(false)
}

fn media_data_root() -> PathBuf {
    if let Some(data_dir) = std::env::var("PRIVCHAT_DATA_DIR")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        return PathBuf::from(data_dir);
    }
    if let Some(home_dir) = std::env::var("HOME")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        return PathBuf::from(home_dir).join(".privchat-rust");
    }
    std::env::temp_dir().join("privchat-rust")
}

fn open_with_system(target: &str) -> Result<(), crate::presentation::vm::UiError> {
    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("open").arg(target).status();
    #[cfg(target_os = "linux")]
    let status = std::process::Command::new("xdg-open").arg(target).status();
    #[cfg(target_os = "windows")]
    let status = std::process::Command::new("cmd")
        .args(["/C", "start", "", target])
        .status();

    status
        .map_err(|e| crate::presentation::vm::UiError::Unknown(format!("spawn opener failed: {e}")))
        .and_then(|s| {
            if s.success() {
                Ok(())
            } else {
                Err(crate::presentation::vm::UiError::Unknown(format!(
                    "open command exited with status: {s}"
                )))
            }
        })
}

fn reveal_in_file_manager(file_path: &str) -> Result<(), crate::presentation::vm::UiError> {
    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("open")
        .args(["-R", file_path])
        .status();
    #[cfg(target_os = "linux")]
    let status = {
        let parent = Path::new(file_path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| file_path.to_string());
        std::process::Command::new("xdg-open").arg(&parent).status()
    };
    #[cfg(target_os = "windows")]
    let status = std::process::Command::new("explorer")
        .args(["/select,", file_path])
        .status();

    status
        .map_err(|e| crate::presentation::vm::UiError::Unknown(format!("spawn file manager failed: {e}")))
        .and_then(|s| {
            if s.success() {
                Ok(())
            } else {
                Err(crate::presentation::vm::UiError::Unknown(format!(
                    "file manager exited with status: {s}"
                )))
            }
        })
}

fn append_runtime_log(state: &mut AppState, level: &str, text: &str) {
    let line = format!(
        "[{}][{}] {}",
        chrono::Local::now().format("%H:%M:%S"),
        level,
        text
    );
    if state.runtime_logs.len() >= MAX_RUNTIME_LOGS {
        let _ = state.runtime_logs.pop_front();
    }
    state.runtime_logs.push_back(line);
}

fn truncate_log_line(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut out = String::with_capacity(max_chars + 3);
    for (index, ch) in value.chars().enumerate() {
        if index >= max_chars {
            break;
        }
        out.push(ch);
    }
    out.push_str("...");
    out
}

fn maybe_play_message_notification_sound(
    state: &mut AppState,
    source: MessageIngressSource,
    message: &MessageVm,
) {
    if !state.settings.notification_sound_enabled {
        return;
    }
    if message.is_own || message.is_deleted || message.server_message_id.is_none() {
        return;
    }
    if source != MessageIngressSource::TimelineUpdated {
        return;
    }
    if let Some(chat) = &state.active_chat {
        if matches!(state.route, Route::Chat)
            && chat.channel_id == message.channel_id
            && chat.channel_type == message.channel_type
        {
            return;
        }
    }

    audio::play_message_notification_sound();
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use iced::Subscription;
    use privchat_sdk::SdkEvent;

    use super::update;
    use crate::app::message::{AppMessage, MessageIngressSource};
    use crate::app::route::Route;
    use crate::app::state::{
        AppState, ChatScreenState, ComposerState, RuntimeMessageIndex, TimelineState,
    };
    use crate::presentation::vm::{
        MessageSendStateVm, MessageVm, PresenceVm, TimelineItemKey, TimelinePatchVm, UiError,
        UnreadMarkerVm,
    };
    use crate::sdk::bridge::SdkBridge;

    #[derive(Clone, Default)]
    struct StubBridge;

    #[async_trait]
    impl SdkBridge for StubBridge {
        fn generate_local_message_id(&self) -> Result<u64, UiError> {
            Ok(1)
        }

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

        async fn sync_channel(
            &self,
            _channel_id: u64,
            _channel_type: i32,
        ) -> Result<usize, UiError> {
            Ok(0)
        }

        async fn list_local_accounts(
            &self,
        ) -> Result<Vec<crate::presentation::vm::LocalAccountVm>, UiError> {
            Ok(Vec::new())
        }

        async fn switch_to_local_account(
            &self,
            _uid: String,
        ) -> Result<crate::presentation::vm::LoginSessionVm, UiError> {
            Err(UiError::Unknown("unused".to_string()))
        }

        async fn load_active_username(&self) -> Result<String, UiError> {
            Ok("demo".to_string())
        }

        async fn logout(&self) -> Result<(), UiError> {
            Ok(())
        }

        async fn search_users(
            &self,
            _query: String,
        ) -> Result<Vec<crate::presentation::vm::SearchUserVm>, UiError> {
            Ok(Vec::new())
        }

        async fn send_friend_request(
            &self,
            _to_user_id: u64,
            _remark: Option<String>,
            _search_session_id: Option<u64>,
        ) -> Result<u64, UiError> {
            Err(UiError::Unknown("unused".to_string()))
        }

        async fn accept_friend_request(&self, _from_user_id: u64) -> Result<u64, UiError> {
            Err(UiError::Unknown("unused".to_string()))
        }

        async fn load_friend_list(
            &self,
        ) -> Result<Vec<crate::presentation::vm::FriendListItemVm>, UiError> {
            Ok(Vec::new())
        }

        async fn batch_get_presence(
            &self,
            _user_ids: Vec<u64>,
        ) -> Result<Vec<PresenceVm>, UiError> {
            Ok(Vec::new())
        }

        async fn load_group_list(
            &self,
        ) -> Result<Vec<crate::presentation::vm::GroupListItemVm>, UiError> {
            Ok(Vec::new())
        }

        async fn load_group_members(
            &self,
            _channel_id: u64,
            _channel_type: i32,
        ) -> Result<Vec<crate::presentation::vm::GroupMemberVm>, UiError> {
            Ok(Vec::new())
        }

        async fn load_friend_request_list(
            &self,
        ) -> Result<Vec<crate::presentation::vm::FriendRequestItemVm>, UiError> {
            Ok(Vec::new())
        }

        async fn load_add_friend_detail(
            &self,
            _item: crate::presentation::vm::AddFriendSelectionVm,
        ) -> Result<crate::presentation::vm::AddFriendDetailVm, UiError> {
            Err(UiError::Unknown("unused".to_string()))
        }

        async fn get_or_create_direct_channel(
            &self,
            _target_user_id: u64,
        ) -> Result<(u64, i32), UiError> {
            Err(UiError::Unknown("unused".to_string()))
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

        async fn subscribe_channel(
            &self,
            _channel_id: u64,
            _channel_type: i32,
        ) -> Result<(), UiError> {
            Ok(())
        }

        async fn send_text_message(
            &self,
            _channel_id: u64,
            _channel_type: i32,
            _client_txn_id: u64,
            _body: String,
            _reply_to_server_message_id: Option<u64>,
            _mentioned_user_ids: Option<Vec<u64>>,
        ) -> Result<u64, UiError> {
            Err(UiError::Unknown("unused".to_string()))
        }

        async fn send_attachment_message(
            &self,
            _channel_id: u64,
            _channel_type: i32,
            _client_txn_id: u64,
            _file_path: String,
        ) -> Result<u64, UiError> {
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

        async fn send_typing(
            &self,
            _channel_id: u64,
            _channel_type: i32,
            _is_typing: bool,
        ) -> Result<(), UiError> {
            Ok(())
        }

        async fn revoke_message(
            &self,
            _channel_id: u64,
            _server_message_id: u64,
        ) -> Result<(), UiError> {
            Ok(())
        }

        async fn delete_message_local(&self, _message_id: u64) -> Result<bool, UiError> {
            Ok(true)
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

        async fn get_file_url(&self, _file_id: u64) -> Result<String, UiError> {
            Err(UiError::Unknown("unused".to_string()))
        }

        async fn get_peer_read_pts(
            &self,
            _channel_id: u64,
            _channel_type: i32,
        ) -> Result<Option<u64>, UiError> {
            Ok(None)
        }

        fn subscribe_timeline(&self, _session_epoch: u64) -> Subscription<SdkEvent> {
            Subscription::none()
        }

        async fn load_user_profile(
            &self,
            _user_id: u64,
            _channel_id: u64,
            _fallback_name: Option<String>,
        ) -> Result<crate::presentation::vm::AddFriendDetailVm, UiError> {
            Err(UiError::Unknown("unused".to_string()))
        }

        async fn set_friend_alias(&self, _user_id: u64, _alias: String) -> Result<bool, UiError> {
            Ok(false)
        }

        async fn pin_channel(&self, _channel_id: u64, _pinned: bool) -> Result<(), UiError> {
            Ok(())
        }

        async fn mute_channel(&self, _channel_id: u64, _muted: bool) -> Result<(), UiError> {
            Ok(())
        }

        async fn delete_friend(&self, _friend_id: u64) -> Result<(), UiError> {
            Ok(())
        }

        async fn add_to_blacklist(&self, _blocked_user_id: u64) -> Result<(), UiError> {
            Ok(())
        }

        async fn remove_from_blacklist(&self, _blocked_user_id: u64) -> Result<(), UiError> {
            Ok(())
        }

        async fn is_user_blacklisted(&self, _user_id: u64) -> Result<bool, UiError> {
            Ok(false)
        }

        async fn fetch_group_members_detailed(
            &self,
            _group_id: u64,
        ) -> Result<Vec<crate::presentation::vm::GroupMemberDetailVm>, UiError> {
            Ok(Vec::new())
        }

        async fn add_group_member(&self, _group_id: u64, _user_id: u64) -> Result<(), UiError> {
            Ok(())
        }

        async fn remove_group_member(&self, _group_id: u64, _user_id: u64) -> Result<(), UiError> {
            Ok(())
        }

        async fn leave_group(&self, _group_id: u64) -> Result<(), UiError> {
            Ok(())
        }

        async fn hide_channel(&self, _channel_id: u64) -> Result<(), UiError> {
            Ok(())
        }

        async fn delete_channel_local(&self, _channel_id: u64) -> Result<(), UiError> {
            Ok(())
        }

        async fn load_quick_phrases(&self) -> Result<Vec<String>, UiError> {
            Ok(Vec::new())
        }

        async fn save_quick_phrases(&self, _phrases: &[String]) -> Result<(), UiError> {
            Ok(())
        }

        async fn start_message_media_download(
            &self,
            _message_id: u64,
            _download_url: String,
            _mime: String,
            _filename_hint: Option<String>,
            _created_at_ms: i64,
        ) -> Result<(), UiError> {
            Ok(())
        }

        async fn pause_message_media_download(&self, _message_id: u64) {}
        async fn resume_message_media_download(&self, _message_id: u64) {}
        async fn cancel_message_media_download(&self, _message_id: u64) {}

        async fn get_media_download_state(
            &self,
            _message_id: u64,
        ) -> privchat_sdk::MediaDownloadState {
            privchat_sdk::MediaDownloadState::Idle
        }

        async fn download_message_media_blocking(
            &self,
            _message_id: u64,
            _download_url: String,
            _mime: String,
            _filename_hint: Option<String>,
            _created_at_ms: i64,
        ) -> Result<String, UiError> {
            Err(UiError::Unknown("unused".to_string()))
        }

        async fn add_reaction(
            &self,
            _server_message_id: u64,
            _channel_id: u64,
            _channel_type: i32,
            _message_id: u64,
            _my_uid: u64,
            _emoji: String,
        ) -> Result<(), UiError> {
            Ok(())
        }

        async fn remove_reaction(
            &self,
            _server_message_id: u64,
            _channel_id: u64,
            _channel_type: i32,
            _message_id: u64,
            _my_uid: u64,
            _emoji: String,
        ) -> Result<(), UiError> {
            Ok(())
        }

        async fn list_local_message_reactions_batch(
            &self,
            _message_ids: Vec<u64>,
            _my_uid: u64,
        ) -> Result<
            std::collections::HashMap<u64, Vec<crate::presentation::vm::ReactionChipVm>>,
            UiError,
        > {
            Ok(std::collections::HashMap::new())
        }

        async fn forward_message(
            &self,
            _src_message_id: u64,
            _target_channel_id: u64,
            _target_channel_type: i32,
        ) -> Result<u64, UiError> {
            Ok(0)
        }
    }

    fn base_state() -> AppState {
        let mut state = AppState::new();
        state.route = Route::Chat;
        state.active_read_channel_id = Some(100);
        state.active_chat = Some(ChatScreenState {
            channel_id: 100,
            channel_type: 2,
            peer_user_id: None,
            title: "测试会话".to_string(),
            open_token: 1,
            timeline: TimelineState::default(),
            runtime_index: RuntimeMessageIndex::default(),
            composer: ComposerState::default(),
            unread_marker: UnreadMarkerVm::default(),
            typing_hint: None,
            typing_user_id: None,

            peer_last_read_pts: None,
            attachment_menu: None,
            user_profile_panel: None,
            last_cursor_pos: None,
            delete_confirm: None,
            message_reactions: std::collections::HashMap::new(),
            reaction_picker_for: None,
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
            media_url: None,
            media_file_id: None,
            media_local_path: None,
            local_thumbnail_path: None,
            thumb_status: 0,
            media_file_size: None,
            voice_duration_secs: None,
            created_at: 0,
            pts: None,
            send_state: Some(MessageSendStateVm::Queued),
            is_own: true,
            is_deleted: false,
            delivered: false,
            reply_to_server_message_id: None,
        }
    }

    fn remote_message(
        message_id: u64,
        server_message_id: u64,
        created_at: i64,
        pts: u64,
        body: &str,
    ) -> MessageVm {
        MessageVm {
            key: TimelineItemKey::Remote { server_message_id },
            channel_id: 100,
            channel_type: 2,
            message_id,
            server_message_id: Some(server_message_id),
            client_txn_id: None,
            from_uid: 42,
            body: body.to_string(),
            message_type: 1,
            media_url: None,
            media_file_id: None,
            media_local_path: None,
            local_thumbnail_path: None,
            thumb_status: 0,
            media_file_size: None,
            voice_duration_secs: None,
            created_at,
            pts: Some(pts),
            send_state: None,
            is_own: false,
            is_deleted: false,
            delivered: false,
            reply_to_server_message_id: None,
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
            media_url: None,
            media_file_id: None,
            media_local_path: None,
            local_thumbnail_path: None,
            thumb_status: 0,
            media_file_size: None,
            voice_duration_secs: None,
            created_at: 0,
            pts: Some(99),
            send_state: None,
            is_own: false,
            is_deleted: false,
            delivered: false,
            reply_to_server_message_id: None,
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
            media_url: None,
            media_file_id: None,
            media_local_path: None,
            local_thumbnail_path: None,
            thumb_status: 0,
            media_file_size: None,
            voice_duration_secs: None,
            created_at: 1,
            pts: Some(22),
            send_state: None,
            is_own: false,
            is_deleted: false,
            delivered: false,
            reply_to_server_message_id: None,
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
    fn global_message_loaded_keeps_timeline_ordered_even_if_arrival_is_reversed() {
        let mut state = base_state();
        let bridge: Arc<dyn SdkBridge> = Arc::new(StubBridge);

        let _ = update(
            &mut state,
            AppMessage::GlobalMessageLoaded {
                message_id: 3003,
                channel_id: Some(100),
                channel_type: Some(2),
                source: MessageIngressSource::TimelineUpdated,
                message: Some(remote_message(3003, 503, 1000, 53, "3")),
            },
            &bridge,
        );
        let _ = update(
            &mut state,
            AppMessage::GlobalMessageLoaded {
                message_id: 3002,
                channel_id: Some(100),
                channel_type: Some(2),
                source: MessageIngressSource::TimelineUpdated,
                message: Some(remote_message(3002, 502, 1000, 52, "2")),
            },
            &bridge,
        );
        let _ = update(
            &mut state,
            AppMessage::GlobalMessageLoaded {
                message_id: 3001,
                channel_id: Some(100),
                channel_type: Some(2),
                source: MessageIngressSource::TimelineUpdated,
                message: Some(remote_message(3001, 501, 1000, 51, "1")),
            },
            &bridge,
        );

        let chat = state.active_chat.as_ref().expect("chat");
        let bodies: Vec<&str> = chat
            .timeline
            .items
            .iter()
            .map(|item| item.body.as_str())
            .collect();
        assert_eq!(bodies, vec!["1", "2", "3"]);
    }

    #[test]
    fn global_incoming_message_increments_unread_when_not_at_bottom() {
        let mut state = base_state();
        let bridge: Arc<dyn SdkBridge> = Arc::new(StubBridge);
        if let Some(chat) = &mut state.active_chat {
            chat.timeline.at_bottom = false;
        }

        let _ = update(
            &mut state,
            AppMessage::GlobalMessageLoaded {
                message_id: 4101,
                channel_id: Some(100),
                channel_type: Some(2),
                source: MessageIngressSource::SubscriptionMessageReceived,
                message: Some(remote_message(4101, 901, 2000, 901, "hello")),
            },
            &bridge,
        );

        let chat = state.active_chat.as_ref().expect("chat");
        assert_eq!(chat.unread_marker.unread_count, 1);
        assert!(chat.unread_marker.has_unread_below_viewport);
        assert_eq!(
            chat.unread_marker.first_unread_key,
            Some(TimelineItemKey::Remote {
                server_message_id: 901
            })
        );
    }

    #[test]
    fn session_list_loaded_does_not_clear_unread_outside_chat_route() {
        let mut state = base_state();
        let bridge: Arc<dyn SdkBridge> = Arc::new(StubBridge);
        state.route = Route::AddFriend;
        state.session_list.items = vec![crate::presentation::vm::SessionListItemVm {
            channel_id: 100,
            channel_type: 2,
            peer_user_id: Some(42),
            title: "demo".to_string(),
            subtitle: "msg".to_string(),
            unread_count: 3,
            last_msg_timestamp: 0,
            is_pinned: false,
            is_muted: false,
        }];
        state.session_list.total_unread_count = 3;

        let _ = update(
            &mut state,
            AppMessage::SessionListLoaded {
                items: vec![crate::presentation::vm::SessionListItemVm {
                    channel_id: 100,
                    channel_type: 2,
                    peer_user_id: Some(42),
                    title: "demo".to_string(),
                    subtitle: "msg".to_string(),
                    unread_count: 3,
                    last_msg_timestamp: 0,
                    is_pinned: false,
                    is_muted: false,
                }],
            },
            &bridge,
        );

        assert_eq!(state.session_list.total_unread_count, 3);
        assert_eq!(
            state.session_list.items.first().map(|it| it.unread_count),
            Some(3)
        );
    }

    #[test]
    fn leaving_chat_page_clears_active_read_context() {
        let mut state = base_state();
        let bridge: Arc<dyn SdkBridge> = Arc::new(StubBridge);
        state.active_read_channel_id = Some(100);

        let _ = update(&mut state, AppMessage::OpenSessionListPage, &bridge);

        assert_eq!(state.route, Route::SessionList);
        assert_eq!(state.active_read_channel_id, None);
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

    #[test]
    fn presence_changed_updates_friend_presence_projection() {
        let mut state = AppState::new();
        let bridge: Arc<dyn SdkBridge> = Arc::new(StubBridge);
        state
            .add_friend
            .friends
            .push(crate::presentation::vm::FriendListItemVm {
                user_id: 42,
                title: "Alice".to_string(),
                subtitle: "UID: 42".to_string(),
                is_added: true,
                is_online: false,
            });

        let _ = update(
            &mut state,
            AppMessage::PresenceChanged {
                presence: PresenceVm {
                    user_id: 42,
                    is_online: true,
                    last_seen_at: 0,
                    device_count: 1,
                },
            },
            &bridge,
        );

        assert_eq!(state.add_friend.friends.len(), 1);
        assert!(state.add_friend.friends[0].is_online);
        assert!(state
            .presences
            .get(&42)
            .map(|p| p.is_online)
            .unwrap_or(false));
    }
}
