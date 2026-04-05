mod app;
mod presentation;
mod sdk;
mod ui;

use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::{window, Element, Font, Size, Subscription, Task};

use app::message::AppMessage;
use app::route::Route;
use app::state::AppState;
use sdk::bridge::{PrivchatSdkBridge, SdkBridge};

/// Root application state. Holds presentation state + sdk bridge.
struct PrivchatApp {
    state: AppState,
    bridge: Arc<dyn SdkBridge>,
}

fn boot() -> (PrivchatApp, Task<AppMessage>) {
    let bridge: Arc<dyn SdkBridge> = Arc::new(PrivchatSdkBridge::new());
    let restore_bridge = Arc::clone(&bridge);
    let app = PrivchatApp {
        state: AppState::new(),
        bridge,
    };
    let restore_task = Task::perform(
        async move {
            let start = Instant::now();
            let session = match restore_bridge.restore_session().await {
                Ok(session) => session,
                Err(error) => {
                    tracing::error!("startup restore_session failed: {:?}", error);
                    None
                }
            };
            let elapsed = start.elapsed();
            let min_splash = Duration::from_millis(600);
            if elapsed < min_splash {
                tokio::time::sleep(min_splash - elapsed).await;
            }
            session
        },
        |session| AppMessage::StartupRestoreCompleted { session },
    );
    (app, restore_task)
}

fn update(app: &mut PrivchatApp, message: AppMessage) -> Task<AppMessage> {
    app::update::update(&mut app.state, message, &app.bridge)
}

fn view(app: &PrivchatApp) -> Element<'_, AppMessage> {
    match app.state.route {
        Route::Splash => ui::screens::splash::view(),
        Route::Login => ui::screens::login::view(&app.state.auth),
        Route::Chat | Route::AddFriend | Route::Settings | Route::SessionList => {
            ui::screens::workspace::view(&app.state)
        }
    }
}

fn subscription(app: &PrivchatApp) -> Subscription<AppMessage> {
    app::subscription::subscription(&app.bridge, &app.state)
}

fn window_title(app: &PrivchatApp) -> String {
    let active_peer_name = app.state.active_chat.as_ref().and_then(|active| {
        app.state
            .session_list
            .items
            .iter()
            .find(|item| {
                item.channel_id == active.channel_id && item.channel_type == active.channel_type
            })
            .map(|item| item.title.trim())
            .filter(|title| !title.is_empty())
    });

    let my_name = if app.state.auth.username.trim().is_empty() {
        app.state
            .auth
            .user_id
            .map(|user_id| format!("U{user_id}"))
            .unwrap_or_else(|| "PrivChat".to_string())
    } else {
        app.state.auth.username.trim().to_string()
    };

    match active_peer_name {
        Some(peer_name) => format!("{peer_name} @ {my_name}"),
        None => "PrivChat".to_string(),
    }
}

fn main() -> iced::Result {
    tracing_subscriber::fmt::init();

    iced::application(boot, update, view)
        .title(window_title)
        .window(window::Settings {
            size: Size::new(1024.0, 768.0),
            min_size: Some(Size::new(800.0, 600.0)),
            ..window::Settings::default()
        })
        .default_font(Font::with_name("Microsoft YaHei"))
        .subscription(subscription)
        .run()
}
