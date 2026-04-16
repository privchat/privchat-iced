use std::path::{Path, PathBuf};

use iced::futures::SinkExt;
use iced::Subscription;
use tokio::net::UnixListener;

use super::message::AppMessage;

fn socket_path(data_dir: &Path) -> PathBuf {
    data_dir.join("privchat.sock")
}

/// Resolve the data directory (same logic as LocalStore / main).
fn data_dir() -> PathBuf {
    std::env::var("PRIVCHAT_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".privchat-rust")
        })
}

/// Try to activate an existing instance via UDS.
/// Returns `true` if the message was sent successfully (another instance is running).
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

/// Seed type for `Subscription::run_with` (must be Hash + Clone + ...).
#[derive(Clone, Debug)]
struct SingleInstanceSeed;

impl std::hash::Hash for SingleInstanceSeed {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        "single-instance-listener".hash(state);
    }
}

fn activate_stream(
    _seed: &SingleInstanceSeed,
) -> impl iced::futures::Stream<Item = AppMessage> {
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

/// Create an iced Subscription that listens on a UDS for "activate" commands
/// from new instances attempting to start.
pub fn listen() -> Subscription<AppMessage> {
    Subscription::run_with(SingleInstanceSeed, activate_stream)
}
