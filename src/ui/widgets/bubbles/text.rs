use iced::widget::text::{LineHeight, Span, Wrapping};
use iced::widget::{rich_text, text};
use iced::{Color, Element};

use crate::app::message::AppMessage;
use crate::presentation::vm::MessageVm;

use super::BubbleCtx;

pub fn view<'a>(message: &'a MessageVm, ctx: &BubbleCtx<'a>) -> Element<'a, AppMessage> {
    let body: &str = &message.body;
    let url_ranges = detect_url_ranges(body);

    if url_ranges.is_empty() {
        // 普通文本：用 `WordOrGlyph` 让超长无空格串（罕见）也能按字形换行，
        // 避免撑出气泡 max_width 后被裁掉。
        return text(body)
            .size(15)
            .line_height(LineHeight::Relative(1.28))
            .wrapping(Wrapping::WordOrGlyph)
            .color(ctx.bubble_text)
            .into();
    }

    let url_color = if ctx.is_own {
        Color::from_rgb8(0x10, 0x4E, 0x8B)
    } else {
        Color::from_rgb8(0x6E, 0xB6, 0xFF)
    };
    let base_color = ctx.bubble_text;

    let mut spans: Vec<Span<'a, String>> = Vec::with_capacity(url_ranges.len() * 2 + 1);
    let mut cursor = 0usize;
    for (start, end) in url_ranges {
        if start > cursor {
            spans.push(Span::new(&body[cursor..start]).color(base_color));
        }
        let url_owned = body[start..end].to_string();
        spans.push(
            Span::new(&body[start..end])
                .color(url_color)
                .underline(true)
                .link(url_owned),
        );
        cursor = end;
    }
    if cursor < body.len() {
        spans.push(Span::new(&body[cursor..]).color(base_color));
    }

    rich_text(spans)
        .size(15)
        .line_height(LineHeight::Relative(1.28))
        .wrapping(Wrapping::WordOrGlyph)
        .on_link_click(AppMessage::OpenExternalUrl)
        .into()
}

/// 极简 URL 检测：仅识别 `http://` 与 `https://` 开头、到下一个 whitespace 为止；
/// 修剪常见的句末标点（含中文）以避免把"。"等纳入链接。
/// 故意不引入额外 crate（linkify/regex）以控制编译体积。
fn detect_url_ranges(body: &str) -> Vec<(usize, usize)> {
    let mut result = Vec::new();
    let mut idx = 0usize;
    while idx < body.len() {
        let suffix = &body[idx..];
        let prefix_len = if suffix.starts_with("https://") {
            8
        } else if suffix.starts_with("http://") {
            7
        } else {
            idx += suffix.chars().next().map(char::len_utf8).unwrap_or(1);
            continue;
        };

        let scheme_end = idx + prefix_len;
        let mut end = scheme_end;
        for (offset, ch) in body[scheme_end..].char_indices() {
            if ch.is_whitespace() || ch.is_control() {
                break;
            }
            end = scheme_end + offset + ch.len_utf8();
        }

        while end > scheme_end {
            let last = body[..end].chars().next_back();
            match last {
                Some(c)
                    if matches!(
                        c,
                        '.' | ',' | ';' | ':' | '!' | '?'
                        | ')' | ']' | '}' | '"' | '\''
                        | '。' | '，' | '；' | '：' | '！' | '？' | '）' | '】'
                    ) =>
                {
                    end -= c.len_utf8();
                }
                _ => break,
            }
        }

        if end > scheme_end {
            result.push((idx, end));
            idx = end;
        } else {
            idx = scheme_end;
        }
    }
    result
}
