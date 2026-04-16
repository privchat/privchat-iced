use std::collections::{HashMap, HashSet, VecDeque};

use iced::widget::{image as iced_image, text_editor};
use iced::window;

use crate::app::auth_prefs;
use crate::app::message::ConnectionTitleState;
use crate::app::route::Route;
use crate::presentation::vm::{
    AddFriendDetailVm, AddFriendSelectionVm, ClientTxnId, FriendListItemVm, FriendRequestItemVm,
    GroupListItemVm, LocalAccountVm, MessageVm, OpenToken, PresenceVm, SearchUserVm,
    SessionListItemVm, TimelineRevision, UnreadMarkerVm,
};

fn default_device_id() -> String {
    if let Some(value) = std::env::var("PRIVCHAT_DEVICE_ID")
        .ok()
        .filter(|value| uuid::Uuid::parse_str(value).is_ok())
    {
        return value;
    }

    uuid::Uuid::new_v4().to_string()
}

#[derive(Debug)]
pub struct AuthState {
    pub username: String,
    pub password: String,
    pub device_id: String,
    pub is_submitting: bool,
    pub error: Option<String>,
    pub user_id: Option<u64>,
    pub token: Option<String>,
}

impl Default for AuthState {
    fn default() -> Self {
        Self {
            username: auth_prefs::load_last_username().unwrap_or_default(),
            password: String::new(),
            device_id: default_device_id(),
            is_submitting: false,
            error: None,
            user_id: None,
            token: None,
        }
    }
}

pub type SessionListItemState = SessionListItemVm;

#[derive(Debug)]
pub struct SessionListState {
    pub items: Vec<SessionListItemState>,
    pub load_error: Option<String>,
    pub total_unread_count: u32,
    /// Set to true when a RefreshSessionList is requested but a load is already in-flight.
    /// Prevents N concurrent list_session loads during a sync burst.
    pub refresh_pending: bool,
    /// True while an async load_session_list call is in-flight.
    pub is_loading: bool,
}

impl Default for SessionListState {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            load_error: None,
            total_unread_count: 0,
            refresh_pending: false,
            is_loading: false,
        }
    }
}

#[derive(Debug)]
pub struct SettingsState {
    pub notification_sound_enabled: bool,
    pub logs_feedback: Option<String>,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            notification_sound_enabled: true,
            logs_feedback: None,
        }
    }
}

#[derive(Debug)]
pub struct AddFriendState {
    pub add_input: String,
    pub search_input: String,
    pub feedback: Option<String>,
    pub search_error: Option<String>,
    pub contacts_error: Option<String>,
    pub search_loading: bool,
    pub search_results: Vec<SearchUserVm>,
    pub selected_search_user_id: Option<u64>,
    pub friends: Vec<FriendListItemVm>,
    pub groups: Vec<GroupListItemVm>,
    pub requests: Vec<FriendRequestItemVm>,
    pub selected_panel_item: Option<AddFriendSelectionVm>,
    pub detail: Option<AddFriendDetailVm>,
    pub detail_loading: bool,
    pub detail_error: Option<String>,
    pub new_friends_expanded: bool,
    pub groups_expanded: bool,
    pub friends_expanded: bool,
}

impl Default for AddFriendState {
    fn default() -> Self {
        Self {
            add_input: String::new(),
            search_input: String::new(),
            feedback: None,
            search_error: None,
            contacts_error: None,
            search_loading: false,
            search_results: Vec::new(),
            selected_search_user_id: None,
            friends: Vec::new(),
            groups: Vec::new(),
            requests: Vec::new(),
            selected_panel_item: None,
            detail: None,
            detail_loading: false,
            detail_error: None,
            new_friends_expanded: false,
            groups_expanded: false,
            friends_expanded: true,
        }
    }
}

#[derive(Debug, Default)]
pub struct OverlayState {
    pub settings_menu_open: bool,
}

#[derive(Debug)]
pub struct SwitchAccountState {
    pub loading: bool,
    pub switching_uid: Option<String>,
    pub accounts: Vec<LocalAccountVm>,
    pub error: Option<String>,
    pub return_route: Route,
    pub add_account_login_mode: bool,
}

impl Default for SwitchAccountState {
    fn default() -> Self {
        Self {
            loading: false,
            switching_uid: None,
            accounts: Vec::new(),
            error: None,
            return_route: Route::SessionList,
            add_account_login_mode: false,
        }
    }
}

pub struct ComposerState {
    pub draft: String,
    pub sending_disabled: bool,
    pub editor: text_editor::Content,
    pub emoji_picker_open: bool,
    pub typing_active: bool,
    pub pending_attachment: Option<PendingAttachmentState>,
}

impl std::fmt::Debug for ComposerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComposerState")
            .field("draft", &self.draft)
            .field("sending_disabled", &self.sending_disabled)
            .field("emoji_picker_open", &self.emoji_picker_open)
            .field("typing_active", &self.typing_active)
            .field("pending_attachment", &self.pending_attachment)
            .finish()
    }
}

impl Default for ComposerState {
    fn default() -> Self {
        Self {
            draft: String::new(),
            sending_disabled: false,
            editor: text_editor::Content::new(),
            emoji_picker_open: false,
            typing_active: false,
            pending_attachment: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PendingAttachmentState {
    pub path: String,
    pub filename: String,
    pub is_image: bool,
}

#[derive(Debug, Default)]
pub struct TimelineState {
    pub revision: TimelineRevision,
    pub items: Vec<MessageVm>,
    pub oldest_server_message_id: Option<u64>,
    pub has_more_before: bool,
    pub is_loading_more: bool,
    pub at_bottom: bool,
}

#[derive(Debug, Default, Clone)]
pub struct RuntimeMessageIndex {
    pub by_message_id: HashMap<u64, ClientTxnId>,
    pub by_client_txn_id: HashMap<ClientTxnId, u64>,
}

impl RuntimeMessageIndex {
    pub fn clear(&mut self) {
        self.by_message_id.clear();
        self.by_client_txn_id.clear();
    }

    pub fn bind(&mut self, message_id: u64, client_txn_id: ClientTxnId) {
        if let Some(previous_message_id) = self.by_client_txn_id.insert(client_txn_id, message_id) {
            self.by_message_id.remove(&previous_message_id);
        }
        self.by_message_id.insert(message_id, client_txn_id);
    }

    pub fn unbind_client_txn_id(&mut self, client_txn_id: ClientTxnId) {
        if let Some(message_id) = self.by_client_txn_id.remove(&client_txn_id) {
            self.by_message_id.remove(&message_id);
        }
    }

    pub fn client_txn_id_for_message(&self, message_id: u64) -> Option<ClientTxnId> {
        self.by_message_id.get(&message_id).copied()
    }

    pub fn message_id_for_client_txn(&self, client_txn_id: ClientTxnId) -> Option<u64> {
        self.by_client_txn_id.get(&client_txn_id).copied()
    }

    pub fn rebuild_from_items(&mut self, items: &[MessageVm]) {
        self.clear();
        for item in items {
            if let Some(client_txn_id) = item.client_txn_id {
                self.bind(item.message_id, client_txn_id);
            }
        }
    }
}

#[derive(Debug)]
pub struct ChatScreenState {
    pub channel_id: u64,
    pub channel_type: i32,
    pub peer_user_id: Option<u64>,
    pub title: String,
    pub open_token: OpenToken,
    pub timeline: TimelineState,
    pub runtime_index: RuntimeMessageIndex,
    pub composer: ComposerState,
    pub unread_marker: UnreadMarkerVm,
    pub typing_hint: Option<String>,
    /// 记录当前正在输入的用户 ID，用于在收到该用户消息时精确清除气泡
    pub typing_user_id: Option<u64>,
    pub peer_last_read_pts: Option<u64>,
    pub attachment_menu: Option<AttachmentMenuState>,
    pub user_profile_panel: Option<UserProfilePanelState>,
}

#[derive(Debug, Clone)]
pub struct UserProfilePanelState {
    pub user_id: u64,
    pub loading: bool,
    pub detail: Option<crate::presentation::vm::AddFriendDetailVm>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ImageViewerState {
    pub message_id: u64,
    pub image_path: String,
    pub loading_original: bool,
    pub original_path: Option<String>,
    pub thumbnail_path: Option<String>,
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct AttachmentMenuState {
    pub message_id: u64,
    pub created_at: i64,
    pub local_path: Option<String>,
    pub file_id: Option<u64>,
    pub filename: String,
    pub copy_text: Option<String>,
}

pub struct AppState {
    pub route: Route,
    pub main_window_id: Option<window::Id>,
    pub add_friend_search_window_id: Option<window::Id>,
    pub logs_window_id: Option<window::Id>,
    pub image_viewer_window_id: Option<window::Id>,
    pub image_viewer: Option<ImageViewerState>,
    pub active_chat: Option<ChatScreenState>,
    pub auth: AuthState,
    pub layout: WorkspaceLayoutState,
    pub session_list: SessionListState,
    pub presences: HashMap<u64, PresenceVm>,
    pub add_friend: AddFriendState,
    pub settings: SettingsState,
    pub overlay: OverlayState,
    pub switch_account: SwitchAccountState,
    pub runtime_logs: VecDeque<String>,
    pub connection_title_state: ConnectionTitleState,
    /// 当前会话激活上下文（v1 仅按 channel_id 判定）。
    /// None 表示"当前不在任何会话阅读态"，所有自动已读逻辑必须失活。
    pub active_read_channel_id: Option<u64>,
    /// Monotonic counter bumped on every account switch / login / restore.
    /// Included in the SDK event subscription hash so Iced recreates the stream.
    pub session_epoch: u64,
    pub media_downloads_inflight: HashSet<u64>,
    /// Decoded RGBA image handles keyed by message_id. Reused across frames.
    pub image_cache: HashMap<u64, iced_image::Handle>,
    /// message_ids currently being decoded asynchronously.
    pub image_decode_pending: HashSet<u64>,
    next_open_token: OpenToken,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("route", &self.route)
            .field("active_chat", &self.active_chat)
            .field("image_cache_len", &self.image_cache.len())
            .field("image_decode_pending", &self.image_decode_pending)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub struct WorkspaceLayoutState {
    pub session_list_width: f32,
    pub is_resizing_session_splitter: bool,
    pub last_cursor_x: Option<f32>,
    pub window_width: f32,
}

impl Default for WorkspaceLayoutState {
    fn default() -> Self {
        Self {
            session_list_width: 260.0,
            is_resizing_session_splitter: false,
            last_cursor_x: None,
            window_width: 1024.0,
        }
    }
}

impl AppState {
    pub fn new() -> Self {
        Self {
            route: Route::default(),
            main_window_id: None,
            add_friend_search_window_id: None,
            logs_window_id: None,
            image_viewer_window_id: None,
            image_viewer: None,
            active_chat: None,
            auth: AuthState::default(),
            layout: WorkspaceLayoutState::default(),
            session_list: SessionListState::default(),
            presences: HashMap::new(),
            add_friend: AddFriendState::default(),
            settings: SettingsState::default(),
            overlay: OverlayState::default(),
            switch_account: SwitchAccountState::default(),
            runtime_logs: VecDeque::new(),
            connection_title_state: ConnectionTitleState::Connecting,
            active_read_channel_id: None,
            session_epoch: 0,
            media_downloads_inflight: HashSet::new(),
            image_cache: HashMap::new(),
            image_decode_pending: HashSet::new(),
            next_open_token: 1,
        }
    }

    pub fn allocate_open_token(&mut self) -> OpenToken {
        let token = self.next_open_token;
        self.next_open_token = self.next_open_token.saturating_add(1);
        token
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
