mod app;
mod audio;
mod config;
mod presentation;
mod sdk;
mod ui;

use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::{window, Element, Font, Size, Subscription, Task};
use tracing_subscriber::EnvFilter;

use app::message::AppMessage;
use app::route::Route;
use app::state::AppState;
use config::AppConfig;
use sdk::bridge::{PrivchatSdkBridge, SdkBridge};

/// Root application state. Holds presentation state + sdk bridge.
struct PrivchatApp {
    state: AppState,
    bridge: Arc<dyn SdkBridge>,
}

fn boot(config: AppConfig) -> (PrivchatApp, Task<AppMessage>) {
    let bridge: Arc<dyn SdkBridge> = Arc::new(PrivchatSdkBridge::new(config));
    let restore_bridge = Arc::clone(&bridge);
    let mut app = PrivchatApp {
        state: AppState::new(),
        bridge,
    };
    let (main_window_id, open_main_window_task) = window::open(main_window_settings());
    app.state.main_window_id = Some(main_window_id);

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
    let open_main_window_task =
        open_main_window_task.map(|window_id| AppMessage::MainWindowOpened { window_id });
    (app, Task::batch([open_main_window_task, restore_task]))
}

fn update(app: &mut PrivchatApp, message: AppMessage) -> Task<AppMessage> {
    app::update::update(&mut app.state, message, &app.bridge)
}

fn view(app: &PrivchatApp, window_id: window::Id) -> Element<'_, AppMessage> {
    if app.state.add_friend_search_window_id == Some(window_id) {
        return ui::screens::add_friend::search_window_view(&app.state.add_friend);
    }
    if app.state.logs_window_id == Some(window_id) {
        let logs = app.state.runtime_logs.iter().cloned().collect::<Vec<_>>();
        let feedback = app.state.settings.logs_feedback.clone();
        return ui::screens::logs::view(logs, feedback);
    }
    if app.state.image_viewer_window_id == Some(window_id) {
        if let Some(viewer) = &app.state.image_viewer {
            return ui::screens::image_viewer::view(viewer);
        }
    }

    match app.state.route {
        Route::Splash => ui::screens::splash::view(),
        Route::Login => ui::screens::login::view(
            &app.state.auth,
            app.state.switch_account.add_account_login_mode,
        ),
        Route::SwitchAccount => ui::screens::switch_account::view(&app.state.switch_account),
        Route::Chat | Route::AddFriend | Route::Settings | Route::SessionList => {
            ui::screens::workspace::view(&app.state)
        }
    }
}

fn subscription(app: &PrivchatApp) -> Subscription<AppMessage> {
    app::subscription::subscription(&app.bridge, &app.state)
}

fn window_title(app: &PrivchatApp, window_id: window::Id) -> String {
    if app.state.add_friend_search_window_id == Some(window_id) {
        return "添加联系人".to_string();
    }
    if app.state.logs_window_id == Some(window_id) {
        return "PrivChat 日志窗口".to_string();
    }
    if app.state.image_viewer_window_id == Some(window_id) {
        return app.state.image_viewer
            .as_ref()
            .map(|v| v.title.clone())
            .unwrap_or_else(|| "图片查看器".to_string());
    }

    let my_name = app.state.auth.username.trim();
    let my_name = if my_name.is_empty() {
        "PrivChat".to_string()
    } else {
        my_name.to_string()
    };

    let active_chat_name = app
        .state
        .active_chat
        .as_ref()
        .and_then(|active| {
            app.state
                .session_list
                .items
                .iter()
                .find(|item| {
                    item.channel_id == active.channel_id && item.channel_type == active.channel_type
                })
                .map(|item| item.title.trim())
                .filter(|title| !title.is_empty())
        })
        .or_else(|| {
            app.state
                .active_chat
                .as_ref()
                .map(|chat| chat.title.trim())
                .filter(|title| !title.is_empty())
        });

    let add_friend_selected_name = app
        .state
        .add_friend
        .detail
        .as_ref()
        .map(|detail| detail.title.trim())
        .filter(|title| !title.is_empty())
        .or_else(|| {
            app.state
                .add_friend
                .selected_panel_item
                .and_then(|selection| match selection {
                    crate::presentation::vm::AddFriendSelectionVm::Friend(user_id) => app
                        .state
                        .add_friend
                        .friends
                        .iter()
                        .find(|item| item.user_id == user_id)
                        .map(|item| item.title.trim())
                        .filter(|title| !title.is_empty()),
                    crate::presentation::vm::AddFriendSelectionVm::Group(group_id) => app
                        .state
                        .add_friend
                        .groups
                        .iter()
                        .find(|item| item.group_id == group_id)
                        .map(|item| item.title.trim())
                        .filter(|title| !title.is_empty()),
                    crate::presentation::vm::AddFriendSelectionVm::Request(user_id) => app
                        .state
                        .add_friend
                        .requests
                        .iter()
                        .find(|item| item.from_user_id == user_id)
                        .map(|item| item.title.trim())
                        .filter(|title| !title.is_empty()),
                })
        });

    let base_title = match app.state.route {
        Route::Chat | Route::SessionList => match active_chat_name {
            Some(peer_name) => format!("{peer_name} @ {my_name}"),
            None => "PrivChat".to_string(),
        },
        Route::AddFriend => match add_friend_selected_name {
            Some(peer_name) => format!("{peer_name} @ {my_name}"),
            None => "联系人".to_string(),
        },
        Route::Settings | Route::Splash | Route::Login | Route::SwitchAccount => {
            "PrivChat".to_string()
        }
    };

    match app.state.connection_title_state {
        app::message::ConnectionTitleState::Disconnected => {
            format!("{base_title}（未连接）")
        }
        app::message::ConnectionTitleState::Connecting => {
            format!("{base_title}（连接中...）")
        }
        app::message::ConnectionTitleState::Connected => base_title,
    }
}

fn main_window_settings() -> window::Settings {
    window::Settings {
        size: Size::new(1024.0, 768.0),
        min_size: Some(Size::new(800.0, 600.0)),
        ..window::Settings::default()
    }
}

fn main() -> anyhow::Result<()> {
    // 配置日志过滤器：默认过滤掉高频低价值日志
    // 可通过 RUST_LOG 环境变量覆盖
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("info")
            .add_directive("refinery_core=warn".parse().unwrap()) // 数据库迁移检查
            .add_directive("msgtrans::transport=warn".parse().unwrap()) // 传输层收发消息
            .add_directive("privchat_iced::sdk::bridge=warn".parse().unwrap()) // presence 轮询
    });

    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    app::reporting::install_report_sink(Arc::new(app::reporting::TracingReportSink));

    let (profile, config) = config::load_app_config()?;

    tracing::info!(
        profile = %profile,
        app_name = %config.application.name,
        "config loaded"
    );

    iced::daemon(move || boot(config.clone()), update, view)
        .title(window_title)
        .default_font(Font::with_name("Microsoft YaHei"))
        .subscription(subscription)
        .run()?;

    Ok(())
}
