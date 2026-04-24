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
    pub is_muted: bool,
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

/// 群聊 @提及候选成员。remark 非空时优先展示，否则使用 display_name。
#[derive(Debug, Clone, Default)]
pub struct GroupMemberVm {
    pub user_id: u64,
    pub display_name: String,
    pub remark: String,
}

impl GroupMemberVm {
    pub fn best_label(&self) -> &str {
        if !self.remark.is_empty() {
            &self.remark
        } else {
            &self.display_name
        }
    }
}

/// 群管理页面的成员条目：比 `GroupMemberVm` 多出角色与加入时间，用于资料页渲染。
/// 角色字符串使用后端约定值：owner / admin / member。
#[derive(Debug, Clone, Default)]
pub struct GroupMemberDetailVm {
    pub user_id: u64,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub role: String,
    pub joined_at_ms: u64,
    pub is_muted: bool,
}

impl GroupMemberDetailVm {
    /// 角色排序值：owner 最优先，admin 次之，其它归为 member。
    pub fn role_rank(&self) -> u8 {
        match self.role.as_str() {
            "owner" => 0,
            "admin" => 1,
            _ => 2,
        }
    }
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
    /// 缩略图状态：0=missing 1=ready 2=failed 3=none（协议层无缩略图）
    pub thumb_status: i32,
    pub media_file_size: Option<u64>,
    /// 语音时长（秒）。仅 Voice（语音消息）类型有效；Audio（音频文件）走文件气泡，不使用该字段。
    pub voice_duration_secs: Option<u32>,
    pub created_at: i64,
    pub pts: Option<u64>,
    pub send_state: Option<MessageSendStateVm>,
    pub is_own: bool,
    pub is_deleted: bool,
    pub delivered: bool,
    /// 引用原消息的 server_message_id（来自 MessagePayloadEnvelope.reply_to_message_id）。
    /// 本地查找不到时渲染"已删除的消息"，不再发起单条拉取。
    pub reply_to_server_message_id: Option<u64>,
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

/// 气泡下方的单个 reaction chip。按 spec 及 privchat-ui `ReactionChip` 对齐：
/// - `emoji`：表情 key（单一字符或组合，UTF-8）
/// - `user_ids`：反应过该 emoji 的用户 id，顺序以最新 seq 优先
/// - `count`：`user_ids.len()`；保留字段便于 UI 不必再算一次
/// - `mine`：当前登录用户是否包含在 `user_ids`
#[derive(Debug, Clone)]
pub struct ReactionChipVm {
    pub emoji: String,
    pub user_ids: Vec<u64>,
    pub count: usize,
    pub mine: bool,
}

/// 默认反应表情（与 privchat-ui `DefaultMessageReactions` 一致）
pub const DEFAULT_REACTION_EMOJIS: &[&str] = &["👍", "❤️", "😂", "🎉", "🔥", "👀"];

/// 转发对话的目标。DirectMessage 携带对方 uid（需先 getOrCreateDirectChannel 解析出
/// channel_id），Group 直接携带群 channel_id。与 privchat-ui `ForwardTarget` 对齐。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ForwardTarget {
    DirectMessage(u64),
    Group(u64),
}

/// 转发对话框中的候选条目。`section` 用于分组展示。
#[derive(Debug, Clone)]
pub struct ForwardTargetVm {
    pub target: ForwardTarget,
    pub title: String,
    pub subtitle: String,
    pub last_msg_timestamp: i64,
}

/// 转发 send 完成后的摘要。
#[derive(Debug, Clone)]
pub struct ForwardSendSummary {
    pub success_count: usize,
    pub failures: Vec<String>,
}

/// 转发对话框上限，与 privchat-ui `ForwardPickerPage` 保持一致。
pub const FORWARD_MAX_TARGETS: usize = 10;
pub const FORWARD_NOTE_MAX: usize = 200;

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
