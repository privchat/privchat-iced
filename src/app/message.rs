use iced::widget::text_editor;
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
    RefreshTotalUnreadCount,
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
    SettingsMenuSwitchAccount,
    SettingsMenuLogout,
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
    RetryOpenConversation {
        channel_id: u64,
        channel_type: i32,
    },

    ComposerInputChanged {
        text: String,
    },
    ToggleEmojiPicker,
    DismissEmojiPicker,
    EmojiPicked {
        emoji: String,
    },
    ComposerPickImagePressed,
    ComposerPickFilePressed,
    ComposerAttachmentPicked {
        path: Option<String>,
    },
    OpenImagePreview {
        message_id: u64,
        local_path: String,
    },
    OpenAttachment {
        message_id: u64,
        local_path: Option<String>,
        file_id: Option<u64>,
        filename: Option<String>,
    },
    ShowAttachmentMenu {
        message_id: u64,
        local_path: Option<String>,
        file_id: Option<u64>,
        filename: String,
    },
    DismissAttachmentMenu,
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
        local_path: Option<String>,
        file_id: Option<u64>,
        filename: String,
        save_path: Option<String>,
    },
    AttachmentSaveAsResolved {
        result: Result<String, UiError>,
    },
    CloseImagePreview,
    MediaThumbnailDownloaded {
        message_id: u64,
        local_path: String,
    },
    MediaThumbnailDownloadFailed {
        message_id: u64,
        error: UiError,
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
    ComposerEdited {
        action: text_editor::Action,
    },
    SendPressed,
    RetrySendPressed {
        channel_id: u64,
        channel_type: i32,
        client_txn_id: ClientTxnId,
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
}
