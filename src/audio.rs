use std::io::Cursor;
use std::thread;

const MESSAGE_NOTIFICATION_SOUND: &[u8] =
    include_bytes!("../assets/message-notification.wav");

pub fn play_message_notification_sound() {
    thread::spawn(|| {
        if let Err(error) = play_embedded_sound(MESSAGE_NOTIFICATION_SOUND) {
            tracing::warn!(?error, "failed to play message notification sound");
        }
    });
}

fn play_embedded_sound(bytes: &'static [u8]) -> anyhow::Result<()> {
    let stream_handle = rodio::DeviceSinkBuilder::open_default_sink()?;
    let player = rodio::play(stream_handle.mixer(), Cursor::new(bytes))?;
    player.sleep_until_end();
    Ok(())
}
