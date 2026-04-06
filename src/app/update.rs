use std::collections::HashSet;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use iced::{window, Size, Task};
use tracing::warn;
use uuid::Uuid;

use crate::app::auth_prefs;
use crate::app::message::{AppMessage, MessageIngressSource};
use crate::app::reporting::{self, TimelinePatchKind};
use crate::app::route::Route;
use crate::app::state::{
    AppState, ChatScreenState, ComposerState, RuntimeMessageIndex, TimelineState,
};
use crate::presentation::vm::{
    AddFriendSelectionVm, ClientTxnId, MessageSendStateVm, MessageVm, OpenToken, TimelineItemKey,
    TimelinePatchVm, UnreadMarkerVm,
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
                    schedule_session_list_refresh(state, bridge),
                    schedule_total_unread_refresh(bridge),
                    schedule_local_accounts_refresh(bridge),
                ]);
            } else {
                state.route = Route::Login;
                state.auth.is_submitting = false;
                state.switch_account.add_account_login_mode = false;
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
                }
            }
            if let Some((channel_id, channel_type)) = state
                .active_chat
                .as_ref()
                .map(|chat| (chat.channel_id, chat.channel_type))
            {
                clear_local_unread_for_channel(state, channel_id, channel_type);
            }
            state.session_list.total_unread_count = state
                .session_list
                .items
                .iter()
                .map(|item| item.unread_count)
                .sum();
            let mut tasks = vec![schedule_total_unread_refresh(bridge)];
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

        AppMessage::RefreshSessionList => schedule_session_list_refresh(state, bridge),

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
            if !matches!(state.route, Route::AddFriend) {
                return Task::none();
            }
            state.add_friend.contacts_error = None;
            schedule_add_friend_refresh(bridge)
        }

        AppMessage::AddFriendFriendsLoaded { items } => {
            state.add_friend.friends = items;
            state.add_friend.contacts_error = None;
            sync_add_friend_flags(state);
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
            state.route = if state.active_chat.is_some() {
                Route::Chat
            } else {
                Route::SessionList
            };
            Task::none()
        }

        AppMessage::MainWindowOpened { window_id } => {
            state.main_window_id = Some(window_id);
            Task::none()
        }

        AppMessage::AddFriendSearchWindowOpened { window_id } => {
            state.add_friend_search_window_id = Some(window_id);
            Task::none()
        }

        AppMessage::OpenAddFriendPage => {
            state.overlay.settings_menu_open = false;
            state.route = Route::AddFriend;
            if state.auth.user_id.is_none() {
                return Task::none();
            }
            state.add_friend.feedback = None;
            state.add_friend.contacts_error = None;
            schedule_add_friend_refresh(bridge)
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
            if state.main_window_id == Some(window_id) {
                return iced::exit();
            }
            if state.add_friend_search_window_id == Some(window_id) {
                state.add_friend_search_window_id = None;
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
            state.route = Route::Settings;
            Task::none()
        }

        AppMessage::SettingsMenuSwitchAccount => {
            state.overlay.settings_menu_open = false;
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

        AppMessage::SwitchAccountSucceeded { uid, session } => {
            apply_logout(state);
            state.auth.username = uid;
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
            ])
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
                normalize_timeline_items(&mut chat.timeline.items);
                chat.timeline.oldest_server_message_id = snapshot.oldest_server_message_id;
                chat.timeline.has_more_before = snapshot.has_more_before;
                chat.unread_marker = snapshot.unread_marker;
                chat.runtime_index.rebuild_from_items(&chat.timeline.items);
            }
            clear_local_unread_for_channel(state, channel_id, channel_type);
            let last_read_pts = state
                .active_chat
                .as_ref()
                .and_then(|chat| latest_read_pts(&chat.timeline.items))
                .unwrap_or(0);
            schedule_mark_read_task(bridge, channel_id, channel_type, last_read_pts)
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

        AppMessage::AddFriendDetailAddFriendPressed { user_id } => {
            let already_friend = state
                .add_friend
                .friends
                .iter()
                .any(|friend| friend.user_id == user_id);
            if already_friend {
                state.add_friend.feedback = Some("该用户已是好友".to_string());
                return Task::none();
            }

            state.add_friend.feedback = Some("发送好友申请中...".to_string());

            let bridge = Arc::clone(bridge);
            Task::perform(
                async move { bridge.send_friend_request(user_id, None, None).await },
                |result| match result {
                    Ok(user_id) => AppMessage::AddFriendRequestSucceeded { user_id },
                    Err(error) => AppMessage::AddFriendRequestFailed { error },
                },
            )
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
        } => handle_global_message_loaded(
            state,
            bridge,
            message_id,
            channel_id,
            channel_type,
            source,
            message,
        ),

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
                normalize_timeline_items(&mut chat.timeline.items);
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
            near_top,
        } => handle_viewport_changed(
            state,
            bridge,
            channel_id,
            channel_type,
            at_bottom,
            near_top,
        ),
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
    state.route = Route::Chat;
    state.active_chat = Some(ChatScreenState {
        channel_id,
        channel_type,
        title: resolved_title,
        open_token,
        timeline: TimelineState::default(),
        runtime_index: RuntimeMessageIndex::default(),
        composer: ComposerState::default(),
        unread_marker: UnreadMarkerVm::default(),
    });
    clear_local_unread_for_channel(state, channel_id, channel_type);

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

fn resolve_chat_title(state: &AppState, channel_id: u64, channel_type: i32) -> String {
    if let Some(item) = state
        .session_list
        .items
        .iter()
        .find(|item| item.channel_id == channel_id && item.channel_type == channel_type)
        .filter(|item| !item.title.trim().is_empty())
    {
        return item.title.clone();
    }

    if let Some(detail) = &state.add_friend.detail {
        let title = detail.title.trim();
        if !title.is_empty() {
            return title.to_string();
        }
    }

    if let Some(selection) = state.add_friend.selected_panel_item {
        return match selection {
            AddFriendSelectionVm::Friend(user_id) | AddFriendSelectionVm::Request(user_id) => {
                format!("UID {user_id}")
            }
            AddFriendSelectionVm::Group(group_id) => format!("群组 {group_id}"),
        };
    }

    format!("会话 {}", channel_id)
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
            message_type: 0,
            created_at: now,
            pts: None,
            send_state: Some(MessageSendStateVm::Sending),
            is_own: true,
            is_deleted: false,
        };
        chat.timeline.items.push(local_echo);
        chat.runtime_index.bind(client_txn_id, client_txn_id);
        chat.composer.draft.clear();
        chat.composer.editor = iced::widget::text_editor::Content::new();
        chat.composer.emoji_picker_open = false;
    }
    touch_session_preview(state, channel_id, channel_type, &body, now);

    let bridge = Arc::clone(bridge);
    Task::perform(
        async move {
            bridge
                .send_text_message(channel_id, channel_type, client_txn_id, body)
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
    )
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
    clear_local_unread_for_channel(state, channel_id, channel_type);
    schedule_mark_read_task(bridge, channel_id, channel_type, last_read_pts)
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
    };

    if applied {
        normalize_timeline_items(&mut chat.timeline.items);
    }
    applied
}

fn normalize_timeline_items(items: &mut [MessageVm]) {
    items.sort_by_key(timeline_order_key);
}

fn timeline_order_key(item: &MessageVm) -> (u8, u64, i64, u64, u64) {
    let tier = if item.pts.is_some() {
        0_u8
    } else if item.server_message_id.is_some() {
        1_u8
    } else {
        2_u8
    };
    (
        tier,
        item.pts.unwrap_or(u64::MAX),
        item.created_at,
        item.server_message_id.unwrap_or(u64::MAX),
        item.message_id,
    )
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
    use crate::app::message::{AppMessage, MessageIngressSource};
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

        async fn load_friend_list(
            &self,
        ) -> Result<Vec<crate::presentation::vm::FriendListItemVm>, UiError> {
            Ok(Vec::new())
        }

        async fn load_group_list(
            &self,
        ) -> Result<Vec<crate::presentation::vm::GroupListItemVm>, UiError> {
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

        async fn send_text_message(
            &self,
            _channel_id: u64,
            _channel_type: i32,
            _client_txn_id: u64,
            _body: String,
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

        fn subscribe_timeline(&self, _session_epoch: u64) -> Subscription<SdkEvent> {
            Subscription::none()
        }
    }

    fn base_state() -> AppState {
        let mut state = AppState::new();
        state.route = Route::Chat;
        state.active_chat = Some(ChatScreenState {
            channel_id: 100,
            channel_type: 2,
            title: "测试会话".to_string(),
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
            created_at,
            pts: Some(pts),
            send_state: None,
            is_own: false,
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
