use iced::widget::text_editor;

use crate::presentation::vm::{
    ClientTxnId, HistoryPageVm, LoginSessionVm, OpenToken, SessionListItemVm, TimelineItemKey,
    TimelinePatchVm, TimelineRevision, TimelineSnapshotVm, UiError,
};

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
    TotalUnreadCountLoaded {
        count: u32,
    },
    TotalUnreadCountLoadFailed {
        error: UiError,
    },
    RefreshSessionList,
    RefreshTotalUnreadCount,
    LoginUsernameChanged {
        text: String,
    },
    LoginPasswordChanged {
        text: String,
    },
    LoginDeviceIdChanged {
        text: String,
    },
    FocusNextWidget,
    FocusPreviousWidget,
    GlobalLeftMousePressed,
    SessionSplitterDragStarted,
    SessionSplitterDragEnded,
    GlobalCursorMoved {
        x: f32,
    },
    WindowResized {
        width: f32,
    },
    OpenSessionListPage,
    OpenAddFriendPage,
    ToggleSettingsMenu,
    DismissSettingsMenu,
    SettingsMenuOpenSettings,
    SettingsMenuLogout,
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
    AddFriendInputChanged {
        text: String,
    },
    AddFriendSearchChanged {
        text: String,
    },
    ToggleNewFriendsSection,
    ToggleGroupSection,
    ToggleFriendSection,
    AddFriendRequestPressed,
    ComposerEdited {
        action: text_editor::Action,
    },
    SendPressed,
    RetrySendPressed {
        channel_id: u64,
        channel_type: i32,
        client_txn_id: ClientTxnId,
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
        first_visible_item: Option<TimelineItemKey>,
    },
}
