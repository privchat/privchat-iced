use std::sync::Arc;

use iced::event;
use iced::keyboard::{self, key, Key};
use iced::mouse;
use iced::window;
use iced::Subscription;

use crate::app::message::AppMessage;
use crate::app::state::AppState;
use crate::sdk::bridge::SdkBridge;
use crate::sdk::events::{self, EventMapContext};

fn map_context_from_state(state: &AppState) -> Option<EventMapContext> {
    let chat = state.active_chat.as_ref()?;
    let mut message_id_to_client_txn = chat
        .runtime_index
        .by_message_id
        .iter()
        .map(|(message_id, client_txn_id)| (*message_id, *client_txn_id))
        .collect::<Vec<_>>();
    message_id_to_client_txn.sort_unstable_by_key(|(message_id, _)| *message_id);

    Some(EventMapContext {
        channel_id: chat.channel_id,
        channel_type: chat.channel_type,
        open_token: chat.open_token,
        message_id_to_client_txn,
    })
}

fn map_global_event(
    event: iced::Event,
    status: event::Status,
    window_id: iced::window::Id,
) -> Option<AppMessage> {
    match event {
        iced::Event::Mouse(mouse::Event::CursorMoved { position }) => {
            Some(AppMessage::GlobalCursorMoved {
                window_id,
                x: position.x,
            })
        }
        iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => match status {
            event::Status::Ignored => Some(AppMessage::GlobalLeftMousePressed { window_id }),
            event::Status::Captured => None,
        },
        iced::Event::Window(window::Event::Resized(size)) => Some(AppMessage::WindowResized {
            window_id,
            width: size.width,
        }),
        iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: Key::Named(key::Named::Tab),
            modifiers,
            ..
        }) => {
            if modifiers.command() || modifiers.control() || modifiers.alt() {
                None
            } else if modifiers.shift() {
                Some(AppMessage::FocusPreviousWidget { window_id })
            } else {
                Some(AppMessage::FocusNextWidget { window_id })
            }
        }
        _ => None,
    }
}

/// Build subscriptions for SDK event streams.
pub fn subscription(bridge: &Arc<dyn SdkBridge>, state: &AppState) -> Subscription<AppMessage> {
    let mut subscriptions = vec![
        event::listen_with(map_global_event),
        window::close_requests().map(|window_id| AppMessage::WindowCloseRequested { window_id }),
    ];
    if let Some(context) = map_context_from_state(state) {
        subscriptions.push(
            bridge
                .subscribe_timeline(state.session_epoch)
                .with(context)
                .map(events::map_sdk_event_with_context),
        );
    } else {
        subscriptions.push(
            bridge
                .subscribe_timeline(state.session_epoch)
                .map(events::map_sdk_event_without_context),
        );
    }

    Subscription::batch(subscriptions)
}
