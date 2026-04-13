use std::io::Cursor;
use std::thread;

use rodio::{Decoder, OutputStream, Sink};

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
    let (_stream, stream_handle) = OutputStream::try_default()?;
    let sink = Sink::try_new(&stream_handle)?;
    let source = Decoder::new(Cursor::new(bytes))?;
    sink.append(source);
    sink.sleep_until_end();
    Ok(())
}
