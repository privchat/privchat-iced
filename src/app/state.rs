use std::collections::HashMap;

use iced::widget::text_editor;

use crate::app::auth_prefs;
use crate::app::route::Route;
use crate::presentation::vm::{
    ClientTxnId, MessageVm, OpenToken, TimelineItemKey, TimelineRevision, UnreadMarkerVm,
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

#[derive(Debug, Default)]
pub struct SessionListItemState {
    pub channel_id: u64,
    pub channel_type: i32,
    pub title: String,
    pub subtitle: String,
}

#[derive(Debug)]
pub struct SessionListState {
    pub items: Vec<SessionListItemState>,
}

impl Default for SessionListState {
    fn default() -> Self {
        Self {
            items: vec![
                SessionListItemState {
                    channel_id: 100,
                    channel_type: 2,
                    title: "Home（自家人）".to_string(),
                    subtitle: "[Channel] JAMES街坊's Activity".to_string(),
                },
                SessionListItemState {
                    channel_id: 101,
                    channel_type: 2,
                    title: "李欣慈".to_string(),
                    subtitle: "我都睡了一觉醒了".to_string(),
                },
                SessionListItemState {
                    channel_id: 102,
                    channel_type: 2,
                    title: "Jolin.刘阿峰".to_string(),
                    subtitle: "👍".to_string(),
                },
                SessionListItemState {
                    channel_id: 103,
                    channel_type: 2,
                    title: "刘若依 山东".to_string(),
                    subtitle: "Voice Call".to_string(),
                },
                SessionListItemState {
                    channel_id: 104,
                    channel_type: 2,
                    title: "游哥".to_string(),
                    subtitle: "Video Call".to_string(),
                },
                SessionListItemState {
                    channel_id: 105,
                    channel_type: 2,
                    title: "威廉".to_string(),
                    subtitle: "恩恩".to_string(),
                },
                SessionListItemState {
                    channel_id: 106,
                    channel_type: 2,
                    title: "玫瑰 海防 胡志明".to_string(),
                    subtitle: "你最近没时间回胡志明吧？".to_string(),
                },
                SessionListItemState {
                    channel_id: 107,
                    channel_type: 2,
                    title: "Jenny 珍妮".to_string(),
                    subtitle: "Voice Call".to_string(),
                },
                SessionListItemState {
                    channel_id: 108,
                    channel_type: 2,
                    title: "乘骑 海南陵水县 法师".to_string(),
                    subtitle: "好".to_string(),
                },
                SessionListItemState {
                    channel_id: 109,
                    channel_type: 2,
                    title: "溪烈 深圳 法师".to_string(),
                    subtitle: "[Channel] 可乐动物园长's Activity".to_string(),
                },
            ],
        }
    }
}

#[derive(Debug, Default)]
pub struct SettingsState;

#[derive(Debug, Default)]
pub struct OverlayState;

pub struct ComposerState {
    pub draft: String,
    pub sending_disabled: bool,
    pub editor: text_editor::Content,
    pub emoji_picker_open: bool,
}

impl std::fmt::Debug for ComposerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComposerState")
            .field("draft", &self.draft)
            .field("sending_disabled", &self.sending_disabled)
            .field("emoji_picker_open", &self.emoji_picker_open)
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
        }
    }
}

#[derive(Debug, Default)]
pub struct TimelineState {
    pub revision: TimelineRevision,
    pub items: Vec<MessageVm>,
    pub oldest_server_message_id: Option<u64>,
    pub has_more_before: bool,
    pub is_loading_more: bool,
    pub first_visible_item: Option<TimelineItemKey>,
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
    pub open_token: OpenToken,
    pub timeline: TimelineState,
    pub runtime_index: RuntimeMessageIndex,
    pub composer: ComposerState,
    pub unread_marker: UnreadMarkerVm,
}

#[derive(Debug)]
pub struct AppState {
    pub route: Route,
    pub active_chat: Option<ChatScreenState>,
    pub auth: AuthState,
    pub layout: WorkspaceLayoutState,
    pub session_list: SessionListState,
    pub settings: SettingsState,
    pub overlay: OverlayState,
    next_open_token: OpenToken,
    next_client_txn_id: ClientTxnId,
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
            session_list_width: 360.0,
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
            active_chat: None,
            auth: AuthState::default(),
            layout: WorkspaceLayoutState::default(),
            session_list: SessionListState::default(),
            settings: SettingsState,
            overlay: OverlayState,
            next_open_token: 1,
            next_client_txn_id: 1,
        }
    }

    pub fn allocate_open_token(&mut self) -> OpenToken {
        let token = self.next_open_token;
        self.next_open_token = self.next_open_token.saturating_add(1);
        token
    }

    pub fn allocate_client_txn_id(&mut self) -> ClientTxnId {
        let id = self.next_client_txn_id;
        self.next_client_txn_id = self.next_client_txn_id.saturating_add(1);
        id
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
