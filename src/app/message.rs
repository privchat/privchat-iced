use iced::widget::text_editor;
use iced::window;

use crate::presentation::vm::{
    AddFriendDetailVm, AddFriendSelectionVm, ClientTxnId, FriendListItemVm, FriendRequestItemVm,
    GroupListItemVm, HistoryPageVm, LoginSessionVm, OpenToken, SearchUserVm, SessionListItemVm,
    TimelineItemKey, TimelinePatchVm, TimelineRevision, TimelineSnapshotVm, UiError,
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
    RefreshAddFriendData,
    AddFriendFriendsLoaded {
        items: Vec<FriendListItemVm>,
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
    LoginUsernameChanged {
        text: String,
    },
    LoginPasswordChanged {
        text: String,
    },
    LoginDeviceIdChanged {
        text: String,
    },
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
