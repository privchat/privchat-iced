use std::path::{Path, PathBuf};

use iced::futures::SinkExt;
use iced::Subscription;

use super::message::AppMessage;

fn socket_path(data_dir: &Path) -> PathBuf {
    data_dir.join("privchat.sock")
}

/// Port file used on Windows to store the TCP port for single-instance detection.
#[cfg(windows)]
fn port_file_path(data_dir: &Path) -> PathBuf {
    data_dir.join("privchat.port")
}

/// Resolve the data directory (same logic as LocalStore / main).
fn data_dir() -> PathBuf {
    std::env::var("PRIVCHAT_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            #[cfg(windows)]
            let home = std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .unwrap_or_else(|_| ".".to_string());
            #[cfg(not(windows))]
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".privchat-rust")
        })
}

/// Try to activate an existing instance.
/// Returns `true` if the message was sent successfully (another instance is running).
#[cfg(not(windows))]
pub fn try_activate_existing() -> bool {
    let sock = socket_path(&data_dir());
    let Ok(stream) = std::os::unix::net::UnixStream::connect(&sock) else {
        return false;
    };
    use std::io::Write;
    let mut stream = stream;
    let _ = stream.write_all(b"activate\n");
    let _ = stream.flush();
    true
}

#[cfg(windows)]
pub fn try_activate_existing() -> bool {
    let dir = data_dir();
    let port_file = port_file_path(&dir);
    let Ok(port_str) = std::fs::read_to_string(&port_file) else {
        return false;
    };
    let Ok(port) = port_str.trim().parse::<u16>() else {
        return false;
    };
    let Ok(mut stream) = std::net::TcpStream::connect(("127.0.0.1", port)) else {
        // Port file is stale; clean it up
        let _ = std::fs::remove_file(&port_file);
        return false;
    };
    use std::io::Write;
    let _ = stream.write_all(b"activate\n");
    let _ = stream.flush();
    true
}

/// Seed type for `Subscription::run_with` (must be Hash + Clone + ...).
#[derive(Clone, Debug)]
struct SingleInstanceSeed;

impl std::hash::Hash for SingleInstanceSeed {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        "single-instance-listener".hash(state);
    }
}

#[cfg(not(windows))]
fn activate_stream(
    _seed: &SingleInstanceSeed,
) -> impl iced::futures::Stream<Item = AppMessage> {
    use tokio::net::UnixListener;

    iced::stream::channel(4, async move |mut output: iced::futures::channel::mpsc::Sender<AppMessage>| {
        let dir = data_dir();
        let sock = socket_path(&dir);

        // Clean up stale socket file (from previous crash)
        let _ = std::fs::remove_file(&sock);

        let listener = match UnixListener::bind(&sock) {
            Ok(l) => l,
            Err(e) => {
                tracing::error!("single-instance: failed to bind UDS {:?}: {}", sock, e);
                std::future::pending::<()>().await;
                unreachable!()
            }
        };

        tracing::info!("single-instance: listening on {:?}", sock);

        loop {
            match listener.accept().await {
                Ok((_stream, _addr)) => {
                    tracing::info!("single-instance: received activate signal, raising window");
                    let _ = output.send(AppMessage::ActivateMainWindow).await;
                }
                Err(e) => {
                    tracing::warn!("single-instance: accept error: {}", e);
                }
            }
        }
    })
}

#[cfg(windows)]
fn activate_stream(
    _seed: &SingleInstanceSeed,
) -> impl iced::futures::Stream<Item = AppMessage> {
    use tokio::net::TcpListener;

    iced::stream::channel(4, async move |mut output: iced::futures::channel::mpsc::Sender<AppMessage>| {
        let dir = data_dir();
        let port_file = port_file_path(&dir);

        // Bind to a random available port on localhost
        let listener = match TcpListener::bind("127.0.0.1:0").await {
            Ok(l) => l,
            Err(e) => {
                tracing::error!("single-instance: failed to bind TCP listener: {}", e);
                std::future::pending::<()>().await;
                unreachable!()
            }
        };

        let port = listener.local_addr().unwrap().port();

        // Write port to file so other instances can find us
        if let Err(e) = std::fs::write(&port_file, port.to_string()) {
            tracing::error!("single-instance: failed to write port file: {}", e);
        }

        tracing::info!("single-instance: listening on 127.0.0.1:{}", port);

        loop {
            match listener.accept().await {
                Ok((_stream, _addr)) => {
                    tracing::info!("single-instance: received activate signal, raising window");
                    let _ = output.send(AppMessage::ActivateMainWindow).await;
                }
                Err(e) => {
                    tracing::warn!("single-instance: accept error: {}", e);
                }
            }
        }
    })
}

/// Create an iced Subscription that listens for "activate" commands
/// from new instances attempting to start.
pub fn listen() -> Subscription<AppMessage> {
    Subscription::run_with(SingleInstanceSeed, activate_stream)
}
