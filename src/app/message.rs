use iced::widget::{image as iced_image, text_editor};
use iced::window;
use privchat_sdk::MediaDownloadState;

use std::collections::HashMap;

use crate::presentation::vm::{
    AddFriendDetailVm, AddFriendSelectionVm, ClientTxnId, ForwardSendSummary, ForwardTarget,
    FriendListItemVm, FriendRequestItemVm, GroupListItemVm, GroupMemberDetailVm, HistoryPageVm,
    LocalAccountVm, LoginSessionVm, OpenToken, PresenceVm, ReactionChipVm, SearchUserVm,
    SessionListItemVm, TimelineItemKey, TimelinePatchVm, TimelineRevision, TimelineSnapshotVm,
    UiError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageIngressSource {
    TimelineUpdated,
    MessageSendStatusChanged,
    OutboundQueueUpdated,
    SubscriptionMessageReceived,
}

impl MessageIngressSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TimelineUpdated => "timeline_updated",
            Self::MessageSendStatusChanged => "message_send_status_changed",
            Self::OutboundQueueUpdated => "outbound_queue_updated",
            Self::SubscriptionMessageReceived => "subscription_message_received",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionTitleState {
    Disconnected,
    Connecting,
    Connected,
}

#[derive(Debug, Clone)]
pub enum AppMessage {
    Noop,
    StartupRestoreCompleted {
        session: Option<LoginSessionVm>,
    },
    SessionListLoaded {
        items: Vec<SessionListItemVm>,
    },
    SessionListLoadFailed {
        error: UiError,
    },
    RefreshAddFriendData,
    AddFriendFriendsLoaded {
        items: Vec<FriendListItemVm>,
    },
    FriendPresencesLoaded {
        items: Vec<PresenceVm>,
    },
    FriendPresencesLoadFailed {
        error: UiError,
    },
    AddFriendFriendsLoadFailed {
        error: UiError,
    },
    AddFriendGroupsLoaded {
        items: Vec<GroupListItemVm>,
    },
    AddFriendGroupsLoadFailed {
        error: UiError,
    },
    AddFriendRequestsLoaded {
        items: Vec<FriendRequestItemVm>,
    },
    AddFriendRequestsLoadFailed {
        error: UiError,
    },
    TotalUnreadCountLoaded {
        count: u32,
    },
    TotalUnreadCountLoadFailed {
        error: UiError,
    },
    RefreshSessionList,
    RefreshPresenceSnapshot,
    ActiveConversationRefreshed {
        channel_id: u64,
        channel_type: i32,
        open_token: OpenToken,
        snapshot: TimelineSnapshotVm,
    },
    ActiveConversationRefreshFailed {
        channel_id: u64,
        channel_type: i32,
        open_token: OpenToken,
        error: UiError,
    },
    RefreshTotalUnreadCount,
    ConnectionTitleStateChanged {
        state: ConnectionTitleState,
    },
    RepairChannelSyncRequested {
        channel_id: u64,
        channel_type: i32,
    },
    RepairChannelSyncSucceeded {
        channel_id: u64,
        channel_type: i32,
        applied: usize,
    },
    RepairChannelSyncFailed {
        channel_id: u64,
        channel_type: i32,
        error: UiError,
    },
    LoginUsernameChanged {
        text: String,
    },
    LoginPasswordChanged {
        text: String,
    },
    LoginDeviceIdChanged {
        text: String,
    },
    LoginBackPressed,
    FocusNextWidget {
        window_id: window::Id,
    },
    FocusPreviousWidget {
        window_id: window::Id,
    },
    GlobalLeftMousePressed {
        window_id: window::Id,
    },
    SessionSplitterDragStarted,
    SessionSplitterDragEnded,
    GlobalCursorMoved {
        window_id: window::Id,
        x: f32,
    },
    WindowResized {
        window_id: window::Id,
        width: f32,
    },
    OpenSessionListPage,
    OpenAddFriendPage,
    OpenAddFriendSearchWindow,
    MainWindowOpened {
        window_id: window::Id,
    },
    ActivateMainWindow,
    AddFriendSearchWindowOpened {
        window_id: window::Id,
    },
    CloseAddFriendSearchWindow,
    WindowCloseRequested {
        window_id: window::Id,
    },
    ToggleSettingsMenu,
    DismissSettingsMenu,
    SettingsMenuOpenSettings,
    SettingsMenuOpenLogs,
    SettingsMenuSwitchAccount,
    SettingsMenuLogout,
    LogsWindowOpened {
        window_id: window::Id,
    },
    CloseLogsWindow,
    ImageViewerWindowOpened {
        window_id: window::Id,
    },
    CloseImageViewerWindow,
    CopyLogsPressed,
    ClearLogsPressed,
    ExportLogsPressed,
    LogsExportSelected {
        save_path: Option<String>,
    },
    ToggleNotificationSound,
    CloseSwitchAccountPanel,
    SwitchAccountListLoaded {
        accounts: Vec<LocalAccountVm>,
    },
    SwitchAccountListLoadFailed {
        error: UiError,
    },
    SwitchAccountPressed {
        uid: String,
    },
    SwitchAccountAddPressed,
    SwitchAccountSucceeded {
        uid: String,
        session: LoginSessionVm,
    },
    ActiveUsernameLoaded {
        username: String,
    },
    ActiveUsernameLoadFailed {
        error: UiError,
    },
    SwitchAccountFailed {
        uid: String,
        error: UiError,
    },
    LoginPressed,
    RegisterPressed,
    LoginSucceeded {
        user_id: u64,
        token: String,
        device_id: String,
    },
    LoginFailed {
        error: UiError,
    },
    ConversationSelected {
        channel_id: u64,
        channel_type: i32,
    },
    SessionListCursorMoved(iced::Point),
    SessionListItemRightClicked {
        channel_id: u64,
        channel_type: i32,
        is_pinned: bool,
    },
    DismissSessionContextMenu,
    PinChannelPressed {
        channel_id: u64,
        channel_type: i32,
        pinned: bool,
    },
    PinChannelResolved {
        channel_id: u64,
        channel_type: i32,
        result: Result<(), UiError>,
    },
    HideChannelPressed {
        channel_id: u64,
        channel_type: i32,
    },
    HideChannelResolved {
        channel_id: u64,
        channel_type: i32,
        result: Result<(), UiError>,
    },
    DeleteChannelPressed {
        channel_id: u64,
        channel_type: i32,
    },
    DeleteChannelResolved {
        channel_id: u64,
        channel_type: i32,
        result: Result<(), UiError>,
    },
    ConversationOpened {
        channel_id: u64,
        channel_type: i32,
        open_token: OpenToken,
        snapshot: TimelineSnapshotVm,
        peer_read_pts: Option<u64>,
    },
    ConversationOpenFailed {
        channel_id: u64,
        channel_type: i32,
        open_token: OpenToken,
        error: UiError,
    },
    ChatPresenceLoaded {
        channel_id: u64,
        channel_type: i32,
        open_token: OpenToken,
        presence: Option<PresenceVm>,
    },
    ChatPresenceLoadFailed {
        channel_id: u64,
        channel_type: i32,
        open_token: OpenToken,
        error: UiError,
    },
    PresenceChanged {
        presence: PresenceVm,
    },
    PeerReadPtsAdvanced {
        channel_id: u64,
        channel_type: i32,
        reader_id: u64,
        read_pts: u64,
    },
    MessageDelivered {
        channel_id: u64,
        channel_type: i32,
        server_message_id: u64,
    },
    TypingStatusChanged {
        channel_id: u64,
        channel_type: i32,
        user_id: u64,
        is_typing: bool,
    },
    TypingHintExpired {
        channel_id: u64,
        channel_type: i32,
        user_id: u64,
    },
    /// 消息到达时清除匹配的 typing 状态
    ClearTypingIfMatch {
        channel_id: u64,
        channel_type: i32,
        user_id: u64,
    },
    TypingSendCompleted {
        is_typing: bool,
    },
    TypingSendFailed {
        is_typing: bool,
        error: UiError,
    },
    RetryOpenConversation {
        channel_id: u64,
        channel_type: i32,
    },

    ComposerInputChanged {
        text: String,
    },
    ComposerPastePressed,
    ToggleEmojiPicker,
    DismissEmojiPicker,
    ToggleQuickPhrase,
    DismissQuickPhrase,
    QuickPhrasePicked { index: usize },
    QuickPhraseDelete { index: usize },
    OpenAddQuickPhrase,
    QuickPhraseInputChanged(String),
    QuickPhraseConfirmAdd,
    QuickPhraseCancelAdd,
    QuickPhraseAdded { phrase: String },
    QuickPhrasesLoaded { phrases: Vec<String> },
    EmojiPicked {
        emoji: String,
    },
    ComposerPickImagePressed,
    ComposerPickFilePressed,
    ComposerAttachmentPicked {
        path: Option<String>,
    },
    ComposerAttachmentSendConfirmed,
    ComposerAttachmentSendCanceled,
    OpenImagePreview {
        message_id: u64,
        /// 原图本地路径（可能不存在）
        original_path: Option<String>,
        /// 缩略图本地路径
        thumbnail_path: Option<String>,
        /// 远程 URL（用于下载）
        media_url: Option<String>,
        /// 文件 ID（用于获取下载链接）
        file_id: Option<u64>,
        /// 消息创建时间（用于构建缓存路径）
        created_at: i64,
    },
    ImageOriginalReady {
        message_id: u64,
        local_path: String,
    },
    ImageOriginalFailed {
        message_id: u64,
        error: UiError,
    },
    MediaDownloadStateChanged {
        message_id: u64,
        state: MediaDownloadState,
    },
    OpenAttachment {
        message_id: u64,
        created_at: i64,
        local_path: Option<String>,
        file_id: Option<u64>,
        filename: Option<String>,
    },
    /// 视频消息：下载（如未缓存）后用系统默认播放器打开。
    OpenVideo {
        message_id: u64,
        created_at: i64,
        local_path: Option<String>,
        file_id: Option<u64>,
        filename: Option<String>,
    },
    ShowAttachmentMenu {
        message_id: u64,
        created_at: i64,
        local_path: Option<String>,
        file_id: Option<u64>,
        filename: String,
    },
    ShowTextMenu {
        message_id: u64,
        text: String,
    },
    /// 进入"引用该消息"草稿态。只对已拿到 server_message_id 的消息可用。
    ReplyToMessagePressed {
        server_message_id: u64,
        from_uid: u64,
        preview: String,
    },
    /// 清除草稿态的引用。
    CancelPendingReply,
    DismissAttachmentMenu,
    ChatCursorMoved(iced::Point),
    TextMenuCopy,
    AttachmentMenuOpen,
    AttachmentMenuOpenFolder,
    AttachmentMenuSaveAs,
    AttachmentOpenResolved {
        result: Result<String, UiError>,
    },
    AttachmentOpenFolderResolved {
        result: Result<String, UiError>,
    },
    AttachmentSaveAsSelected {
        message_id: u64,
        created_at: i64,
        local_path: Option<String>,
        file_id: Option<u64>,
        filename: String,
        save_path: Option<String>,
    },
    AttachmentSaveAsResolved {
        result: Result<String, UiError>,
    },
    CloseImagePreview,
    OpenUserProfile {
        user_id: u64,
    },
    UserProfileLoaded {
        user_id: u64,
        detail: AddFriendDetailVm,
    },
    UserProfileLoadFailed {
        user_id: u64,
        error: UiError,
    },
    CloseUserProfile,
    StartEditAlias,
    AliasInputChanged(String),
    ConfirmEditAlias,
    CancelEditAlias,
    AliasSetResult { success: bool, alias: String },
    MediaThumbnailDownloaded {
        message_id: u64,
        local_path: String,
    },
    MediaThumbnailDownloadFailed {
        message_id: u64,
        error: UiError,
    },
    ImageDecoded {
        message_id: u64,
        handle: iced_image::Handle,
    },
    ImageDecodeFailed {
        message_id: u64,
    },
    AddFriendInputChanged {
        text: String,
    },
    AddFriendSearchChanged {
        text: String,
    },
    AddFriendSearchPressed,
    AddFriendSearchLoaded {
        users: Vec<SearchUserVm>,
    },
    AddFriendSearchFailed {
        error: UiError,
    },
    AddFriendResultSelected {
        user_id: u64,
    },
    AddFriendPanelSelected {
        item: AddFriendSelectionVm,
    },
    AddFriendDetailLoaded {
        item: AddFriendSelectionVm,
        detail: AddFriendDetailVm,
    },
    AddFriendDetailLoadFailed {
        item: AddFriendSelectionVm,
        error: UiError,
    },
    AddFriendDetailSendMessagePressed {
        user_id: u64,
    },
    AddFriendOpenConversationResolved {
        user_id: u64,
        channel_id: u64,
        channel_type: i32,
    },
    AddFriendOpenConversationFailed {
        user_id: u64,
        error: UiError,
    },
    AddFriendDetailAcceptRequestPressed {
        user_id: u64,
    },
    AddFriendAcceptSucceeded {
        user_id: u64,
    },
    AddFriendAcceptFailed {
        user_id: u64,
        error: UiError,
    },
    ToggleNewFriendsSection,
    ToggleGroupSection,
    ToggleFriendSection,
    AddFriendRequestPressed,
    AddFriendRequestSucceeded {
        user_id: u64,
    },
    AddFriendRequestFailed {
        error: UiError,
    },
    CopyDetailFieldPressed {
        label: String,
        value: String,
    },
    ComposerEdited {
        action: text_editor::Action,
    },
    /// 群聊成员列表加载完毕：用于为 @ 提及选择器提供候选。
    MentionMembersLoaded {
        channel_id: u64,
        members: Vec<crate::presentation::vm::GroupMemberVm>,
    },
    /// 点击 MentionPicker 中某一项：把 `@name ` 写回 composer 并记入 span。
    MentionPickerPicked {
        user_id: u64,
    },
    /// 显式关闭 MentionPicker（Esc / 失焦 / 非群聊等）。
    MentionPickerDismissed,
    SendPressed,
    RetrySendPressed {
        channel_id: u64,
        channel_type: i32,
        client_txn_id: ClientTxnId,
    },
    RevokeMessagePressed {
        channel_id: u64,
        channel_type: i32,
        server_message_id: u64,
    },
    RevokeMessageSucceeded {
        server_message_id: u64,
    },
    RevokeMessageFailed {
        server_message_id: u64,
        error: UiError,
    },
    /// 用户点击「本地删除」菜单项，弹出二次确认对话框。
    RequestDeleteMessageLocal {
        channel_id: u64,
        channel_type: i32,
        open_token: OpenToken,
        message_id: u64,
        key: TimelineItemKey,
        preview: String,
    },
    /// 关闭「本地删除」确认对话框（用户取消，或再次按下关闭键）。
    CancelDeleteMessageLocal,
    DeleteMessageLocalPressed {
        channel_id: u64,
        channel_type: i32,
        open_token: OpenToken,
        message_id: u64,
        key: TimelineItemKey,
    },
    DeleteMessageLocalResolved {
        channel_id: u64,
        channel_type: i32,
        open_token: OpenToken,
        key: TimelineItemKey,
        result: Result<bool, UiError>,
    },
    /// 展开 / 收起气泡 reaction 选择条。`None` 代表收起。
    ToggleReactionPicker {
        message_id: Option<u64>,
    },
    /// 用户点击某表情：若 mine 则走 remove，否则走 add。调度 `ReactionToggleResolved`。
    ToggleReactionPressed {
        channel_id: u64,
        channel_type: i32,
        open_token: OpenToken,
        message_id: u64,
        server_message_id: u64,
        emoji: String,
        currently_mine: bool,
    },
    /// RPC + 本地写入完成后回调，用于刷新聚合数据或在失败时回滚。
    ReactionToggleResolved {
        channel_id: u64,
        channel_type: i32,
        open_token: OpenToken,
        message_id: u64,
        emoji: String,
        was_add: bool,
        result: Result<(), UiError>,
    },
    /// 请求批量重建当前活动频道所有可见消息的 reaction 聚合。
    RequestReactionsRefresh {
        channel_id: u64,
        channel_type: i32,
        open_token: OpenToken,
    },
    /// 外部同步通道上报 `message_reaction` 实体变化；update 侧基于 active_chat 触发 refresh。
    SyncMessageReactionChanged,
    /// 批量 reaction 聚合结果已加载：按 message_id 覆盖 state.active_chat.message_reactions。
    ReactionsBatchLoaded {
        channel_id: u64,
        channel_type: i32,
        open_token: OpenToken,
        map: HashMap<u64, Vec<ReactionChipVm>>,
    },
    /// 打开转发对话框。携带源消息的关键信息；update 侧从 session_list / add_friend 快照
    /// 填充候选列表，分配独立 OpenToken 标识本次会话。
    OpenForwardPicker {
        channel_id: u64,
        channel_type: i32,
        message_id: u64,
        server_message_id: Option<u64>,
        preview: String,
    },
    /// 关闭转发对话框（点击取消、背景、或发送成功）。
    DismissForwardPicker,
    /// 搜索框文本变化。
    ForwardSearchChanged(String),
    /// 勾选/取消勾选一个目标。超过上限时忽略新增。
    ForwardTargetToggled(ForwardTarget),
    /// 备注输入框文本变化，超过上限时截断。
    ForwardNoteChanged(String),
    /// 点击「发送」按钮，进入 submitting 态并调度后端任务。
    ForwardSendPressed,
    /// 后端任务完成：按每个目标的结果汇总 success/failures。
    ForwardSendResolved {
        open_token: OpenToken,
        result: Result<ForwardSendSummary, UiError>,
    },

    /// 打开好友设置面板。update 侧分配新 open_token，并调度 mute/block/channel_id 初始加载。
    OpenFriendSettings {
        user_id: u64,
        title: String,
        avatar: String,
        remark: String,
    },
    /// 关闭好友设置面板（取消、背景、或删除成功）。
    DismissFriendSettings,
    /// 初始化结果：direct channel id 解析 + 本地 channel.mute + 本地黑名单命中。
    FriendSettingsLoaded {
        open_token: OpenToken,
        direct_channel_id: u64,
        is_muted: bool,
        is_blacklisted: bool,
    },
    FriendSettingsLoadFailed {
        open_token: OpenToken,
        error: UiError,
    },
    /// 备注编辑相关。
    FriendSettingsRemarkEditPressed,
    FriendSettingsRemarkEditCancelled,
    FriendSettingsRemarkInputChanged(String),
    FriendSettingsRemarkSubmitPressed,
    FriendSettingsRemarkResolved {
        open_token: OpenToken,
        result: Result<String, UiError>,
    },
    /// 免打扰 switch 被切换，携带目标状态。
    FriendSettingsMuteToggled(bool),
    FriendSettingsMuteResolved {
        open_token: OpenToken,
        muted: bool,
        result: Result<(), UiError>,
    },
    /// 拉黑 switch 被切换。
    FriendSettingsBlockToggled(bool),
    FriendSettingsBlockResolved {
        open_token: OpenToken,
        blocked: bool,
        result: Result<(), UiError>,
    },
    /// 点「删除联系人」——打开二次确认。
    FriendSettingsDeletePressed,
    /// 取消二次确认。
    FriendSettingsDeleteCancelled,
    /// 二次确认「确定」——调度 delete_friend。
    FriendSettingsDeleteConfirmed,
    FriendSettingsDeleteResolved {
        open_token: OpenToken,
        result: Result<(), UiError>,
    },

    /// 打开群管理资料页。update 侧分配 open_token，并调度成员详情拉取。
    OpenGroupSettings {
        group_id: u64,
        title: String,
    },
    /// 关闭群管理资料页（取消、背景点击、或退群成功）。
    DismissGroupSettings,
    /// 成员详情拉取完成。
    GroupSettingsLoaded {
        open_token: OpenToken,
        members: Vec<GroupMemberDetailVm>,
        my_role: Option<String>,
    },
    GroupSettingsLoadFailed {
        open_token: OpenToken,
        error: UiError,
    },
    /// 邀请输入框文本变更。
    GroupSettingsInviteInputChanged(String),
    /// 「邀请」按钮按下，校验数字 user_id 后发起 RPC。
    GroupSettingsInviteSubmitPressed,
    GroupSettingsInviteResolved {
        open_token: OpenToken,
        invited_user_id: u64,
        result: Result<(), UiError>,
    },
    /// 单行「移除」被按下。
    GroupSettingsRemoveMemberPressed(u64),
    GroupSettingsRemoveMemberResolved {
        open_token: OpenToken,
        user_id: u64,
        result: Result<(), UiError>,
    },
    /// 点「退出群组」打开二次确认。
    GroupSettingsLeavePressed,
    GroupSettingsLeaveCancelled,
    GroupSettingsLeaveConfirmed,
    GroupSettingsLeaveResolved {
        open_token: OpenToken,
        result: Result<(), UiError>,
    },

    GlobalMessageIngress {
        message_id: u64,
        channel_id: Option<u64>,
        channel_type: Option<i32>,
        source: MessageIngressSource,
    },
    GlobalMessageLoaded {
        message_id: u64,
        channel_id: Option<u64>,
        channel_type: Option<i32>,
        source: MessageIngressSource,
        message: Option<crate::presentation::vm::MessageVm>,
    },
    GlobalMessageLoadFailed {
        message_id: u64,
        channel_id: Option<u64>,
        channel_type: Option<i32>,
        source: MessageIngressSource,
        error: UiError,
    },
    TimelineUpdatedIngress {
        channel_id: u64,
        channel_type: i32,
        open_token: OpenToken,
        message_id: u64,
    },

    TimelinePatched {
        channel_id: u64,
        channel_type: i32,
        open_token: OpenToken,
        revision: TimelineRevision,
        patch: TimelinePatchVm,
    },
    LoadOlderTriggered {
        channel_id: u64,
        channel_type: i32,
    },
    HistoryLoaded {
        channel_id: u64,
        channel_type: i32,
        open_token: OpenToken,
        page: HistoryPageVm,
    },
    HistoryLoadFailed {
        channel_id: u64,
        channel_type: i32,
        open_token: OpenToken,
        error: UiError,
    },

    ViewportChanged {
        channel_id: u64,
        channel_type: i32,
        at_bottom: bool,
        near_top: bool,
    },
    /// 点击语音气泡的播放/停止按钮。若当前正在播放同一条则切为停止。
    VoiceTogglePressed {
        message_id: u64,
        created_at: i64,
        local_path: Option<String>,
        file_id: Option<u64>,
    },
    /// 语音播放自然结束或被主动停止，清理 AppState.voice_playback。
    VoicePlaybackFinished {
        message_id: u64,
        result: Result<(), UiError>,
    },
}
