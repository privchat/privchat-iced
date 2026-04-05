/// Top-level navigation route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Splash,
    Login,
    SessionList,
    Chat,
    Settings,
}

impl Default for Route {
    fn default() -> Self {
        Self::Splash
    }
}
