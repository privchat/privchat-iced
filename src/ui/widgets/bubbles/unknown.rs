use iced::widget::text;
use iced::Element;

use crate::app::message::AppMessage;

use super::BubbleCtx;

pub fn view<'a>(ctx: &BubbleCtx<'a>) -> Element<'a, AppMessage> {
    text("[未知类型消息]")
        .size(15)
        .line_height(iced::widget::text::LineHeight::Relative(1.28))
        .color(ctx.bubble_text)
        .into()
}
