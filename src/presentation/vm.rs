use privchat_protocol::message::ContentMessageType;

/// UI-only identities (not from SDK)
pub type ClientTxnId = u64;
pub type OpenToken = u64;
pub type TimelineRevision = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiError {
    Unknown(String),
}

impl Default for UiError {
    fn default() -> Self {
        Self::Unknown("unknown".to_string())
    }
}

#[derive(Debug, Clone, Default)]
pub struct LoginSessionVm {
    pub user_id: u64,
    pub token: String,
    pub device_id: String,
}

#[derive(Debug, Clone, Default)]
pub struct LocalAccountVm {
    pub uid: String,
    pub is_active: bool,
    pub created_at: i64,
    pub last_login_at: i64,
}

#[derive(Debug, Clone, Default)]
pub struct SessionListItemVm {
    pub channel_id: u64,
    pub channel_type: i32,
    pub peer_user_id: Option<u64>,
    pub title: String,
    pub subtitle: String,
    pub unread_count: u32,
    pub last_msg_timestamp: i64,
    pub is_pinned: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SearchUserVm {
    pub user_id: u64,
    pub username: String,
    pub nickname: String,
    pub avatar_url: Option<String>,
    pub user_type: i16,
    pub search_session_id: u64,
    pub is_friend: bool,
    pub can_send_message: bool,
}

#[derive(Debug, Clone, Default)]
pub struct FriendListItemVm {
    pub user_id: u64,
    pub title: String,
    pub subtitle: String,
    pub is_added: bool,
    pub is_online: bool,
}

#[derive(Debug, Clone, Default)]
pub struct PresenceVm {
    pub user_id: u64,
    pub is_online: bool,
    pub last_seen_at: i64,
    pub device_count: u32,
}

#[derive(Debug, Clone, Default)]
pub struct GroupListItemVm {
    pub group_id: u64,
    pub title: String,
    pub subtitle: String,
}

#[derive(Debug, Clone, Default)]
pub struct FriendRequestItemVm {
    pub from_user_id: u64,
    pub user: SearchUserVm,
    pub title: String,
    pub subtitle: String,
    pub is_added: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddFriendSelectionVm {
    Friend(u64),
    Group(u64),
    Request(u64),
}

#[derive(Debug, Clone, Default)]
pub struct AddFriendDetailFieldVm {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Default)]
pub struct AddFriendDetailVm {
    pub title: String,
    pub subtitle: String,
    pub fields: Vec<AddFriendDetailFieldVm>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimelineItemKey {
    Local(ClientTxnId),
    Remote { server_message_id: u64 },
}

impl Default for TimelineItemKey {
    fn default() -> Self {
        Self::Local(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageSendStateVm {
    Queued,
    Sending,
    Sent,
    FailedRetryable { reason: UiError },
    FailedPermanent { reason: UiError },
    Retrying,
}

#[derive(Debug, Clone, Default)]
pub struct MessageVm {
    pub key: TimelineItemKey,
    pub channel_id: u64,
    pub channel_type: i32,
    pub message_id: u64,
    pub server_message_id: Option<u64>,
    pub client_txn_id: Option<ClientTxnId>,
    pub from_uid: u64,
    pub body: String,
    pub message_type: i32,
    pub media_url: Option<String>,
    pub media_file_id: Option<u64>,
    pub media_local_path: Option<String>,
    pub local_thumbnail_path: Option<String>,
    pub media_file_size: Option<u64>,
    /// 语音时长（秒）。仅 Voice（语音消息）类型有效；Audio（音频文件）走文件气泡，不使用该字段。
    pub voice_duration_secs: Option<u32>,
    pub created_at: i64,
    pub pts: Option<u64>,
    pub send_state: Option<MessageSendStateVm>,
    pub is_own: bool,
    pub is_deleted: bool,
    pub delivered: bool,
}

impl MessageVm {
    /// 原始 i32 `message_type` 归一化成协议枚举。负数或未知 u32 返回 `None`
    /// （保留"未识别类型兜底"的语义，避免新增协议类型时客户端 panic）。
    pub fn content_type(&self) -> Option<ContentMessageType> {
        u32::try_from(self.message_type)
            .ok()
            .and_then(ContentMessageType::from_u32)
    }
}

#[derive(Debug, Clone, Default)]
pub struct TimelineSnapshotVm {
    pub revision: TimelineRevision,
    pub items: Vec<MessageVm>,
    pub oldest_server_message_id: Option<u64>,
    pub has_more_before: bool,
    pub unread_marker: UnreadMarkerVm,
}

#[derive(Debug, Clone, Default)]
pub struct HistoryPageVm {
    pub items: Vec<MessageVm>,
    pub oldest_server_message_id: Option<u64>,
    pub has_more_before: bool,
}

#[derive(Debug, Clone, Default)]
pub struct UnreadMarkerVm {
    pub first_unread_key: Option<TimelineItemKey>,
    pub unread_count: u32,
    pub has_unread_below_viewport: bool,
}

#[derive(Debug, Clone)]
pub enum TimelinePatchVm {
    ReplaceLocalEcho {
        client_txn_id: ClientTxnId,
        remote: MessageVm,
    },
    UpsertRemote {
        remote: MessageVm,
    },
    UpdateSendState {
        client_txn_id: ClientTxnId,
        send_state: MessageSendStateVm,
    },
    RemoveMessage {
        key: TimelineItemKey,
    },
    UpdateUnreadMarker {
        unread_marker: UnreadMarkerVm,
    },
}
