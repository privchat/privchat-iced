use iced::widget::{image as iced_image, text_editor};
use iced::window;

use crate::presentation::vm::{
    AddFriendDetailVm, AddFriendSelectionVm, ClientTxnId, FriendListItemVm, FriendRequestItemVm,
    GroupListItemVm, HistoryPageVm, LocalAccountVm, LoginSessionVm, OpenToken, PresenceVm,
    SearchUserVm, SessionListItemVm, TimelinePatchVm, TimelineRevision, TimelineSnapshotVm,
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
    OpenAttachment {
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
    DismissAttachmentMenu,
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
