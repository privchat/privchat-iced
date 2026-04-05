use iced::Element;

use crate::app::message::AppMessage;
use crate::app::state::SettingsState;

/// Render the settings screen.
pub fn view(_settings: &SettingsState) -> Element<'_, AppMessage> {
    iced::widget::text("Settings placeholder").into()
}
