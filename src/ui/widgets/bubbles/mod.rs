use std::collections::HashMap;

use iced::{Color, Element};
use privchat_protocol::message::ContentMessageType;

use crate::app::message::AppMessage;
use crate::presentation::vm::MessageVm;

pub mod file;
pub mod image;
pub mod text;
pub mod unknown;
pub mod video;
pub mod voice;

/// 本地 UI 显示类型。协议/服务端不感知，仅用于渲染分派：
/// - `Revoked`：`is_deleted` 状态 overlay 的归一化
/// - `System`：协议 `ContentMessageType::System`
/// - `Bubble`：其余走常规气泡（内部按 `content_type()` 再次细分）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRenderType {
    Revoked,
    System,
    Bubble,
}

pub fn render_type(message: &MessageVm) -> MessageRenderType {
    if message.is_deleted {
        MessageRenderType::Revoked
    } else if matches!(message.content_type(), Some(ContentMessageType::System)) {
        MessageRenderType::System
    } else {
        MessageRenderType::Bubble
    }
}

/// 常规气泡分派到的具体子类型（仅在 `MessageRenderType::Bubble` 分支内有意义）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BubbleKind {
    Text,
    Image,
    Video,
    File,
    Voice,
    Unknown,
}

impl BubbleKind {
    /// 右键会弹"打开 / 所在目录 / 另存为"菜单的子类型。
    /// Voice 由行内播放按钮直接消费，不走附件菜单。
    pub fn is_attachment(self) -> bool {
        matches!(self, Self::Image | Self::Video | Self::File)
    }
}

pub struct BubbleCtx<'a> {
    pub bubble_text: Color,
    pub is_own: bool,
    pub render_media_preview: bool,
    pub image_cache: &'a HashMap<u64, iced::widget::image::Handle>,
    /// 当前正在播放的语音消息 id；用于语音气泡切换 ▶/■ 图标。
    pub playing_voice_message_id: Option<u64>,
}

pub struct BubbleContent<'a> {
    pub element: Element<'a, AppMessage>,
    pub kind: BubbleKind,
}

/// 根据 `content_type()` 分派到对应气泡模块。未识别 u32 / 未支持类型统一走 Unknown。
pub fn render<'a>(message: &'a MessageVm, ctx: &BubbleCtx<'a>) -> BubbleContent<'a> {
    match message.content_type() {
        Some(ContentMessageType::Text) => BubbleContent {
            element: text::view(message, ctx),
            kind: BubbleKind::Text,
        },
        Some(ContentMessageType::Image) => BubbleContent {
            element: image::view(message, ctx),
            kind: BubbleKind::Image,
        },
        Some(ContentMessageType::Video) => BubbleContent {
            element: video::view(message, ctx),
            kind: BubbleKind::Video,
        },
        Some(ContentMessageType::File) => BubbleContent {
            element: file::view(message, ctx),
            kind: BubbleKind::File,
        },
        Some(ContentMessageType::Voice) => BubbleContent {
            element: voice::view(message, ctx),
            kind: BubbleKind::Voice,
        },
        _ => BubbleContent {
            element: unknown::view(ctx),
            kind: BubbleKind::Unknown,
        },
    }
}
