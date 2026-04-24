use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc;

use iced::widget::{image as iced_image, text_editor};
use iced::window;

use crate::app::auth_prefs;
use crate::app::message::ConnectionTitleState;
use crate::app::route::Route;
use crate::presentation::vm::{
    AddFriendDetailVm, AddFriendSelectionVm, ClientTxnId, ForwardTarget, FriendListItemVm,
    FriendRequestItemVm, GroupListItemVm, GroupMemberDetailVm, LocalAccountVm, MessageVm,
    OpenToken, PresenceVm, ReactionChipVm, SearchUserVm, SessionListItemVm, TimelineRevision,
    UnreadMarkerVm,
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
    /// 右键菜单状态。None 表示没有打开的菜单。
    pub context_menu: Option<SessionContextMenuState>,
    /// 会话面板内最近一次光标位置（相对面板左上角），供右键菜单定位。
    pub last_cursor_pos: Option<iced::Point>,
}

impl Default for SessionListState {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            load_error: None,
            total_unread_count: 0,
            refresh_pending: false,
            is_loading: false,
            context_menu: None,
            last_cursor_pos: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SessionContextMenuState {
    pub channel_id: u64,
    pub channel_type: i32,
    pub is_pinned: bool,
    pub anchor_pos: Option<iced::Point>,
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
    pub quick_phrase_open: bool,
    pub quick_phrases: Vec<String>,
    pub quick_phrase_adding: bool,
    pub quick_phrase_input: String,
    pub typing_active: bool,
    pub pending_attachment: Option<PendingAttachmentState>,
    pub pending_reply: Option<PendingReplyState>,
    /// 群聊成员缓存（只在 channel_type=2 的会话进入时加载一次）。
    pub group_members: Vec<crate::presentation::vm::GroupMemberVm>,
    /// 已落入正文的 @ 提及 span（字节偏移，端开区间）。
    pub mentions: Vec<MentionSpan>,
    /// 提及选择器；`visible=false` 但 `query=Some` 表示匹配不到候选。
    pub mention_picker: Option<MentionPickerState>,
}

/// 一段 `@name ` 的区间记录（端开区间），`end` 含尾随空格，用于原子删除与偏移追踪。
/// start/end 都是 UTF-8 字节下标，便于和 Rust 字符串切片对齐。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MentionSpan {
    pub start: usize,
    pub end: usize,
    pub user_id: u64,
}

/// @ 提及选择器状态。`query` 为裸查询串（不含 `@`），`filtered` 为排序后命中的候选列表。
#[derive(Debug, Clone)]
pub struct MentionPickerState {
    pub query: String,
    pub filtered: Vec<crate::presentation::vm::GroupMemberVm>,
}

#[derive(Debug, Clone)]
pub struct PendingReplyState {
    pub server_message_id: u64,
    pub from_uid: u64,
    pub preview: String,
}

impl std::fmt::Debug for ComposerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComposerState")
            .field("draft", &self.draft)
            .field("sending_disabled", &self.sending_disabled)
            .field("emoji_picker_open", &self.emoji_picker_open)
            .field("quick_phrase_open", &self.quick_phrase_open)
            .field("quick_phrases_count", &self.quick_phrases.len())
            .field("typing_active", &self.typing_active)
            .field("pending_attachment", &self.pending_attachment)
            .field("pending_reply", &self.pending_reply)
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
            quick_phrase_open: false,
            quick_phrases: Vec::new(),
            quick_phrase_adding: false,
            quick_phrase_input: String::new(),
            typing_active: false,
            pending_attachment: None,
            pending_reply: None,
            group_members: Vec::new(),
            mentions: Vec::new(),
            mention_picker: None,
        }
    }
}

// ==================== @ 提及工具函数 ====================
// 端口自 privchat-ui MessagePage.kt (lines 2465-2548)。
//
// 设计决定：
// - span 用 **UTF-8 字节下标** 而非字符下标。text_editor::Content 的 perform + text()
//   只给 String，没有 cursor 信息；byte 位置和 String 切片一致，便于 diff 推断。
// - `compute_mention_query` 检测尾部的 `@query`，要求 `@` 位于行首或紧邻空白，
//   过滤邮箱等场景。
// - `resolve_mention_edit` 用前后缀 diff 取出变更区间；任何与现有 span 相交的编辑
//   都按整段删除处理，给用户 "一次 backspace 抹掉整条 @mention" 的手感。

/// 推断输入尾部的 @ 查询串：最后一个 `@` 必须在行首或紧随空白，且其后不含空白。
/// 返回的字符串 *不包含* `@` 前缀。DM 会话直接返回 None。
pub fn compute_mention_query(text: &str, is_dm: bool) -> Option<String> {
    if is_dm {
        return None;
    }
    let at_idx = text.rfind('@')?;
    if at_idx > 0 {
        let before = &text[..at_idx];
        let last_char = before.chars().next_back()?;
        if !last_char.is_whitespace() {
            return None;
        }
    }
    let tail = &text[at_idx + 1..];
    if tail.chars().any(|c| c.is_whitespace()) {
        return None;
    }
    Some(tail.to_string())
}

/// 把输入尾部的 `@query` 替换为 `@<name> `（保留触发符、附带空格）。
/// 返回 (新文本, 新增 span)。
pub fn replace_mention_query(text: &str, name: &str, user_id: u64) -> (String, MentionSpan) {
    let at_idx = text.rfind('@').unwrap_or(text.len());
    let prefix = &text[..at_idx];
    let new_text = format!("{prefix}@{name} ");
    let span_start = prefix.len();
    let span_end = new_text.len();
    (new_text, MentionSpan { start: span_start, end: span_end, user_id })
}

/// 头像长按 / picker 显式追加：若末尾不是空白，先补空格。
pub fn append_mention(text: &str, name: &str, user_id: u64) -> (String, MentionSpan) {
    let needs_space = !text.is_empty()
        && text.chars().next_back().map(|c| !c.is_whitespace()).unwrap_or(false);
    let prefix: String = if needs_space {
        format!("{text} ")
    } else {
        text.to_string()
    };
    let span_start = prefix.len();
    let new_text = format!("{prefix}@{name} ");
    let span_end = new_text.len();
    (new_text, MentionSpan { start: span_start, end: span_end, user_id })
}

/// 把用户编辑后的文本与旧文本/旧 span 做 diff 合并：
/// - 编辑未触碰任何 span → 原样应用，只把后续 span 按 delta 平移。
/// - 编辑落入某个 span（哪怕咬一口）→ 把整段 span 从 *旧文本* 中摘掉，本次编辑一并丢弃。
/// 返回 (归并后的文本, 幸存 span 列表)。
pub fn resolve_mention_edit(
    old_text: &str,
    new_text: &str,
    old_spans: &[MentionSpan],
) -> (String, Vec<MentionSpan>) {
    if old_text == new_text {
        return (new_text.to_string(), old_spans.to_vec());
    }
    let old_bytes = old_text.as_bytes();
    let new_bytes = new_text.as_bytes();
    let min_len = old_bytes.len().min(new_bytes.len());
    let mut p = 0usize;
    while p < min_len && old_bytes[p] == new_bytes[p] {
        p += 1;
    }
    let mut s = 0usize;
    while s < min_len - p
        && old_bytes[old_bytes.len() - 1 - s] == new_bytes[new_bytes.len() - 1 - s]
    {
        s += 1;
    }
    let change_end_old = old_bytes.len() - s;
    let delta = new_bytes.len() as isize - old_bytes.len() as isize;

    let damaged: Vec<MentionSpan> = old_spans
        .iter()
        .copied()
        .filter(|span| span.end > p && span.start < change_end_old)
        .collect();

    if damaged.is_empty() {
        let shifted: Vec<MentionSpan> = old_spans
            .iter()
            .map(|span| {
                if span.end <= p {
                    *span
                } else {
                    MentionSpan {
                        start: (span.start as isize + delta) as usize,
                        end: (span.end as isize + delta) as usize,
                        user_id: span.user_id,
                    }
                }
            })
            .collect();
        return (new_text.to_string(), shifted);
    }

    let mut output = old_text.to_string();
    let mut by_desc = damaged.clone();
    by_desc.sort_by(|a, b| b.start.cmp(&a.start));
    for span in &by_desc {
        if span.start <= output.len() && span.end <= output.len() {
            output.replace_range(span.start..span.end, "");
        }
    }
    let survivors: Vec<MentionSpan> = old_spans
        .iter()
        .copied()
        .filter(|span| !damaged.contains(span))
        .map(|span| {
            let removed_before: usize = damaged
                .iter()
                .filter(|d| d.end <= span.start)
                .map(|d| d.end - d.start)
                .sum();
            MentionSpan {
                start: span.start - removed_before,
                end: span.end - removed_before,
                user_id: span.user_id,
            }
        })
        .collect();

    (output, survivors)
}

/// 在 name/remark 上做不区分大小写的包含匹配。
pub fn match_member_query(member: &crate::presentation::vm::GroupMemberVm, query: &str) -> bool {
    let q = query.to_lowercase();
    if q.is_empty() {
        return true;
    }
    member.display_name.to_lowercase().contains(&q)
        || member.remark.to_lowercase().contains(&q)
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
    /// 最近一次在聊天正文内追踪到的光标位置（相对正文 mouse_area 左上角）。
    /// 用于右键菜单按触发位置悬浮渲染。
    pub last_cursor_pos: Option<iced::Point>,
    /// 本地删除消息的二次确认弹窗。Some 时渲染模态对话框。
    pub delete_confirm: Option<DeleteConfirmState>,
    /// 每条消息（key 为 `MessageVm.message_id`）对应的聚合 reaction chips。
    /// 由本地 reaction 表聚合而来；新消息 / 同步事件触发重建。
    pub message_reactions: HashMap<u64, Vec<ReactionChipVm>>,
    /// 当前正在展开 reaction 选择条的消息 id（右键触发或长按）。
    /// 任一时刻最多一条消息展开。
    pub reaction_picker_for: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct DeleteConfirmState {
    pub channel_id: u64,
    pub channel_type: i32,
    pub open_token: crate::presentation::vm::OpenToken,
    pub message_id: u64,
    pub message_key: crate::presentation::vm::TimelineItemKey,
    pub preview: String,
}

#[derive(Debug, Clone)]
pub struct UserProfilePanelState {
    pub user_id: u64,
    pub loading: bool,
    pub detail: Option<crate::presentation::vm::AddFriendDetailVm>,
    pub error: Option<String>,
    pub editing_alias: bool,
    pub alias_input: String,
    pub alias_old_title: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ImageViewerState {
    pub message_id: u64,
    pub image_path: String,
    pub loading_original: bool,
    pub original_path: Option<String>,
    pub thumbnail_path: Option<String>,
    pub title: String,
    /// Current download progress as (bytes, total). Populated while the SDK
    /// download is in flight; cleared once the original is ready.
    pub download_progress: Option<(u64, Option<u64>)>,
}

#[derive(Debug, Clone)]
pub struct AttachmentMenuState {
    pub message_id: u64,
    pub channel_id: u64,
    pub channel_type: i32,
    pub open_token: crate::presentation::vm::OpenToken,
    pub message_key: crate::presentation::vm::TimelineItemKey,
    pub server_message_id: Option<u64>,
    pub is_own: bool,
    pub is_revoked: bool,
    pub is_attachment: bool,
    pub send_state: Option<crate::presentation::vm::MessageSendStateVm>,
    pub created_at: i64,
    pub local_path: Option<String>,
    pub file_id: Option<u64>,
    pub filename: String,
    pub copy_text: Option<String>,
    /// 右键触发时鼠标相对聊天正文的位置，用于浮层定位。
    pub anchor_pos: Option<iced::Point>,
    /// 消息发送者 uid，用于构造引用草稿。
    pub from_uid: u64,
    /// 引用浏览时展示的预览文本（文本气泡用正文，附件气泡用文件名）。
    pub reply_preview: String,
}

/// 全局好友设置面板状态。Some 时作为模态覆盖在任何路由之上。
///
/// 功能对齐 privchat-ui FriendSettingsPage：
/// - 编辑备注
/// - 免打扰开关（调 `mute_channel`，要求 direct_channel_id 已解析）
/// - 拉黑开关（调 `add/remove_to_blacklist`）
/// - 删除好友（二次确认后调 `delete_friend`）
///
/// 特别关注、分享能力因后端缺对应 RPC，当前版本暂不展示。
#[derive(Debug, Clone)]
pub struct FriendSettingsState {
    pub open_token: OpenToken,
    pub user_id: u64,
    /// 顶部展示名（remark > nickname > username），在备注变更成功后会跟进刷新。
    pub title: String,
    /// 头像 URL。
    pub avatar: String,
    /// 当前备注。空字符串代表未设置。
    pub remark: String,
    /// 该私聊对应的 channel_id（mute 需要）。打开时异步解析/创建。
    pub direct_channel_id: Option<u64>,
    /// 初次加载（解析 channel + 拉 mute/block 状态）是否在进行中。
    pub loading: bool,
    pub is_muted: bool,
    pub is_blacklisted: bool,
    pub editing_remark: bool,
    pub remark_input: String,
    pub submitting_remark: bool,
    pub submitting_mute: bool,
    pub submitting_block: bool,
    pub delete_confirm_open: bool,
    pub submitting_delete: bool,
    pub error: Option<String>,
}

/// 全局群管理资料页状态：成员列表 + 邀请/移除/退出群。
#[derive(Debug)]
pub struct GroupSettingsState {
    pub open_token: OpenToken,
    pub group_id: u64,
    pub title: String,
    /// 当前登录用户的 UID，用于判断“是否是我自己”以屏蔽自删除操作。
    pub my_user_id: u64,
    pub loading: bool,
    pub members: Vec<GroupMemberDetailVm>,
    /// 当前登录用户在群内的角色（owner/admin/member）。非成员时为 None。
    pub my_role: Option<String>,
    /// 邀请输入框：支持输入 user_id（数字）。MVP 先不做搜索选人。
    pub invite_input: String,
    pub submitting_invite: bool,
    /// 正在被移除的 user_id（展示行内 loading 态，同时屏蔽重复点击）。
    pub submitting_remove: Option<u64>,
    /// “退出群组”按钮的二次确认是否弹出。
    pub leave_confirm_open: bool,
    pub submitting_leave: bool,
    pub error: Option<String>,
}

impl GroupSettingsState {
    pub fn is_admin(&self) -> bool {
        matches!(self.my_role.as_deref(), Some("owner") | Some("admin"))
    }
}

/// 全局转发对话框状态。Some 时作为模态覆盖在任何路由之上。
#[derive(Debug)]
pub struct ForwardPickerState {
    /// 分配的独立 token，用于区分"两次打开"——结果回调回来时若 token 不匹配则丢弃。
    pub open_token: OpenToken,
    /// 源消息所在的会话与服务端 id。
    pub source_channel_id: u64,
    pub source_channel_type: i32,
    pub source_message_id: u64,
    pub source_server_message_id: Option<u64>,
    /// 顶部展示的源消息预览（文本正文或附件文件名）。
    pub source_preview: String,
    /// 搜索关键字，大小写不敏感过滤标题/昵称。
    pub search: String,
    /// 最近会话，按 last_msg_timestamp 降序。排除群已解散的会话与当前源会话可见。
    pub recent_sessions: Vec<SessionListItemVm>,
    /// 好友列表快照，去重掉已出现在 recent_sessions 的。
    pub friends: Vec<FriendListItemVm>,
    /// 群列表快照，去重掉已出现在 recent_sessions 的。
    pub groups: Vec<GroupListItemVm>,
    /// 已选中目标，保持点击顺序。
    pub selected: Vec<ForwardTarget>,
    /// 可选的备注文本，发送成功后追加一条 text 消息。
    pub note: String,
    /// 正在提交。期间按钮禁用。
    pub submitting: bool,
    /// 失败反馈文案。
    pub error: Option<String>,
}

impl ForwardPickerState {
    pub fn is_selected(&self, target: ForwardTarget) -> bool {
        self.selected.iter().any(|t| *t == target)
    }
}

pub struct AppState {
    pub route: Route,
    pub main_window_id: Option<window::Id>,
    pub add_friend_search_window_id: Option<window::Id>,
    pub logs_window_id: Option<window::Id>,
    pub image_viewer_window_id: Option<window::Id>,
    pub image_viewer: Option<ImageViewerState>,
    pub active_chat: Option<ChatScreenState>,
    /// 全局转发对话框。Some 时模态渲染。
    pub forward_picker: Option<ForwardPickerState>,
    /// 全局好友设置面板。Some 时模态渲染。
    pub friend_settings: Option<FriendSettingsState>,
    /// 全局群管理资料页。Some 时模态渲染。
    pub group_settings: Option<GroupSettingsState>,
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
    /// 当前语音播放句柄；None 表示无在播。切换/结束时清空。
    pub voice_playback: Option<VoicePlaybackHandle>,
    next_open_token: OpenToken,
}

/// 单个语音播放实例：持有停止信号通道，向 audio 线程发 `()` 即可中止。
pub struct VoicePlaybackHandle {
    pub message_id: u64,
    pub stop_tx: mpsc::Sender<()>,
}

impl std::fmt::Debug for VoicePlaybackHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VoicePlaybackHandle")
            .field("message_id", &self.message_id)
            .finish()
    }
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
            forward_picker: None,
            friend_settings: None,
            group_settings: None,
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
            voice_playback: None,
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

#[cfg(test)]
mod mention_helper_tests {
    use super::*;

    #[test]
    fn compute_mention_query_returns_none_in_dm() {
        assert!(compute_mention_query("hi @bob", true).is_none());
    }

    #[test]
    fn compute_mention_query_detects_trailing_at() {
        assert_eq!(compute_mention_query("hi @bo", false).as_deref(), Some("bo"));
        assert_eq!(compute_mention_query("@bo", false).as_deref(), Some("bo"));
    }

    #[test]
    fn compute_mention_query_ignores_email_like() {
        // `@` 前有非空白字符 → 视为 email/普通文本
        assert!(compute_mention_query("foo@bar", false).is_none());
    }

    #[test]
    fn compute_mention_query_ignores_whitespace_in_query() {
        assert!(compute_mention_query("hi @bob is", false).is_none());
    }

    #[test]
    fn replace_mention_query_swaps_in_name_and_tracks_span() {
        let (text, span) = replace_mention_query("hi @bo", "bob", 42);
        assert_eq!(text, "hi @bob ");
        assert_eq!(span.user_id, 42);
        // start 应指向 `@` 字节位置
        assert_eq!(&text[span.start..span.end], "@bob ");
    }

    #[test]
    fn append_mention_pads_space_when_needed() {
        let (text, _) = append_mention("hi", "bob", 1);
        assert_eq!(text, "hi @bob ");
        let (text, _) = append_mention("hi ", "bob", 1);
        assert_eq!(text, "hi @bob ");
        let (text, _) = append_mention("", "bob", 1);
        assert_eq!(text, "@bob ");
    }

    #[test]
    fn resolve_mention_edit_atomic_delete_on_touched_span() {
        // Start: "hi @bob ", span covers @bob[ ] (bytes 3..8)
        let old = "hi @bob ";
        let span = MentionSpan { start: 3, end: 8, user_id: 42 };
        // User backspaces once → last char dropped
        let new_text = "hi @bob";
        let (merged, survivors) = resolve_mention_edit(old, new_text, &[span]);
        // atomic delete: "hi @bob " → "hi "
        assert_eq!(merged, "hi ");
        assert!(survivors.is_empty());
    }

    #[test]
    fn resolve_mention_edit_shifts_later_spans() {
        let old = "hi @a ";
        let span_a = MentionSpan { start: 3, end: 6, user_id: 1 };
        // User appends "x"
        let new_text = "hi @a x";
        let (merged, survivors) = resolve_mention_edit(old, new_text, &[span_a]);
        assert_eq!(merged, new_text);
        assert_eq!(survivors.len(), 1);
        assert_eq!(survivors[0], span_a);
    }
}
