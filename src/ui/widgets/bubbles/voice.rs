use iced::widget::{button, container, row, svg, text, Space};
use iced::{alignment, border, Background, Color, Element, Length, Theme};

use crate::app::message::AppMessage;
use crate::presentation::vm::MessageVm;

use super::BubbleCtx;

// 微信风格：左/右倒置的语音气泡，包含播放/停止图标 + 三条波形条 + 时长数字。
// 整块区域点击触发 VoiceTogglePressed；气泡父层负责左右对齐与背景色。
pub fn view<'a>(message: &'a MessageVm, ctx: &BubbleCtx<'a>) -> Element<'a, AppMessage> {
    let is_playing = ctx.playing_voice_message_id == Some(message.message_id);
    let duration_secs = message.voice_duration_secs.unwrap_or(0).max(1);
    let duration_label = format!("{}\u{2033}", duration_secs);

    // 时长越长气泡越宽，封顶避免过长消息撑破上限。
    let body_width = (84.0 + (duration_secs as f32 * 4.0).min(128.0)).min(220.0);

    let icon = playback_icon(is_playing, ctx.bubble_text, ctx.is_own);
    let bars = wave_bars(ctx.bubble_text, is_playing, ctx.is_own);
    let duration = text(duration_label).size(13).color(ctx.bubble_text);

    // is_own（右侧气泡）: [duration] [spacer] [bars] [icon]
    // 对端（左侧气泡）: [icon] [bars] [spacer] [duration]
    let inner: Element<'_, AppMessage> = if ctx.is_own {
        row![
            duration,
            Space::new().width(Length::Fill),
            bars,
            icon,
        ]
    } else {
        row![
            icon,
            bars,
            Space::new().width(Length::Fill),
            duration,
        ]
    }
    .spacing(8)
    .align_y(alignment::Vertical::Center)
    .width(Length::Fixed(body_width))
    .into();

    button(inner)
        .padding(0)
        .style(navless_button_style)
        .on_press(AppMessage::VoiceTogglePressed {
            message_id: message.message_id,
            created_at: message.created_at,
            local_path: message.media_local_path.clone(),
            file_id: message.media_file_id,
        })
        .into()
}

fn playback_icon<'a>(
    is_playing: bool,
    color: Color,
    is_own: bool,
) -> Element<'a, AppMessage> {
    // 播放中 → 方块；空闲 → 朝向"波形侧"的三角（is_own 波形在左，三角指向左）。
    let bytes: &'static [u8] = if is_playing {
        ICON_STOP
    } else if is_own {
        ICON_PLAY_LEFT
    } else {
        ICON_PLAY_RIGHT
    };
    let handle = svg::Handle::from_memory(bytes);
    svg(handle)
        .width(Length::Fixed(14.0))
        .height(Length::Fixed(14.0))
        .style(move |_theme: &Theme, _status| svg::Style { color: Some(color) })
        .into()
}

fn wave_bars<'a>(color: Color, is_playing: bool, is_own: bool) -> Element<'a, AppMessage> {
    // 三条波形条：中间高、两侧低。播放时提高对比度；未播放时显示半透明让气泡更"静态"。
    let alpha = if is_playing { 1.0 } else { 0.72 };
    let bar_color = Color { a: color.a * alpha, ..color };
    let mut heights = [6.0_f32, 12.0, 8.0];
    if is_own {
        // 右侧气泡波形在左：把高度顺序翻转，视觉上让"高条"靠近扬声器方向。
        heights.reverse();
    }
    row![
        bar(heights[0], bar_color),
        bar(heights[1], bar_color),
        bar(heights[2], bar_color),
    ]
    .spacing(3)
    .align_y(alignment::Vertical::Center)
    .into()
}

fn bar<'a>(height: f32, color: Color) -> Element<'a, AppMessage> {
    container(Space::new())
        .width(Length::Fixed(2.5))
        .height(Length::Fixed(height))
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(color)),
            border: border::rounded(1.25),
            ..container::Style::default()
        })
        .into()
}

fn navless_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => {
            Color::from_rgba8(0x00, 0x00, 0x00, 0.08)
        }
        _ => Color::TRANSPARENT,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::TRANSPARENT,
        border: border::rounded(4.0),
        shadow: Default::default(),
        snap: true,
    }
}

static ICON_PLAY_RIGHT: &[u8] = br#"<svg viewBox='0 0 24 24' xmlns='http://www.w3.org/2000/svg'><path d='M7 5.2 19 12 7 18.8V5.2Z' fill='currentColor'/></svg>"#;
static ICON_PLAY_LEFT: &[u8] = br#"<svg viewBox='0 0 24 24' xmlns='http://www.w3.org/2000/svg'><path d='M17 5.2 5 12l12 6.8V5.2Z' fill='currentColor'/></svg>"#;
static ICON_STOP: &[u8] = br#"<svg viewBox='0 0 24 24' xmlns='http://www.w3.org/2000/svg'><rect x='6' y='6' width='12' height='12' rx='1.5' fill='currentColor'/></svg>"#;
