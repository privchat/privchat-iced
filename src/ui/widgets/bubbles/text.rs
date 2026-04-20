use iced::widget::text;
use iced::Element;

use crate::app::message::AppMessage;
use crate::presentation::vm::MessageVm;

use super::BubbleCtx;

pub fn view<'a>(message: &'a MessageVm, ctx: &BubbleCtx<'a>) -> Element<'a, AppMessage> {
    text(&message.body)
        .size(15)
        .line_height(iced::widget::text::LineHeight::Relative(1.28))
        .color(ctx.bubble_text)
        .into()
}
