/// Top-level navigation route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Splash,
    Login,
    SwitchAccount,
    SessionList,
    Chat,
    AddFriend,
    Settings,
}

impl Default for Route {
    fn default() -> Self {
        Self::Splash
    }
}
