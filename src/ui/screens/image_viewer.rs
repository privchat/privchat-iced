use iced::widget::{button, column, container, image, row, stack, text};
use iced::{alignment, border, Background, Color, Element, Length};

use crate::app::message::AppMessage;
use crate::app::state::ImageViewerState;

const C_BG: Color = Color::from_rgb8(0x12, 0x14, 0x18);
const C_TOOLBAR_BG: Color = Color::from_rgb8(0x1A, 0x1D, 0x22);
const C_BORDER: Color = Color::from_rgb8(0x2A, 0x2E, 0x34);
const C_TEXT: Color = Color::from_rgb8(0xE3, 0xE8, 0xEE);
const C_TEXT_DIM: Color = Color::from_rgb8(0x8B, 0x93, 0x9E);
const C_OVERLAY: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.55);

fn format_progress(bytes: u64, total: Option<u64>) -> String {
    fn human(b: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = 1024 * 1024;
        if b >= MB {
            format!("{:.1}MB", b as f64 / MB as f64)
        } else if b >= KB {
            format!("{:.1}KB", b as f64 / KB as f64)
        } else {
            format!("{}B", b)
        }
    }
    match total {
        Some(total) if total > 0 => {
            let pct = (bytes as f64 / total as f64 * 100.0).clamp(0.0, 100.0);
            format!("{} / {} ({:.0}%)", human(bytes), human(total), pct)
        }
        _ => human(bytes),
    }
}

pub fn view(viewer: &ImageViewerState) -> Element<'_, AppMessage> {
    let toolbar = {
        let title = text(&viewer.title)
            .size(14)
            .color(C_TEXT);

        let status: Element<'_, AppMessage> = if viewer.loading_original {
            let label = match viewer.download_progress {
                Some((bytes, total)) => format!("正在下载原图 {}", format_progress(bytes, total)),
                None => "正在加载原图...".to_string(),
            };
            text(label).size(12).color(C_TEXT_DIM).into()
        } else if viewer.original_path.is_some() {
            text("原图")
                .size(12)
                .color(Color::from_rgb8(0x22, 0xC5, 0x5E))
                .into()
        } else {
            text("缩略图")
                .size(12)
                .color(C_TEXT_DIM)
                .into()
        };

        container(
            row![title, container(text("")).width(Length::Fill), status]
                .spacing(12)
                .align_y(alignment::Vertical::Center),
        )
        .padding([10, 16])
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(C_TOOLBAR_BG)),
            border: border::width(0.0)
                .color(C_BORDER)
                .rounded(0.0),
            ..container::Style::default()
        })
    };

    let image_area: Element<'_, AppMessage> = if !viewer.image_path.is_empty() {
        let base = container(
            image(viewer.image_path.clone())
                .width(Length::Fill)
                .height(Length::Fill)
                .content_fit(iced::ContentFit::Contain),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(12)
        .style(|_| container::Style {
            background: Some(Background::Color(C_BG)),
            ..container::Style::default()
        });

        if viewer.loading_original {
            let progress_label = match viewer.download_progress {
                Some((bytes, total)) => format_progress(bytes, total),
                None => "加载中".to_string(),
            };
            let overlay = container(
                column![
                    text("正在下载原图").size(14).color(C_TEXT),
                    text(progress_label).size(12).color(C_TEXT_DIM),
                ]
                .spacing(6)
                .align_x(alignment::Horizontal::Center),
            )
            .padding([10, 18])
            .style(|_| container::Style {
                background: Some(Background::Color(C_OVERLAY)),
                border: border::rounded(8.0),
                text_color: Some(C_TEXT),
                ..container::Style::default()
            });
            let overlay_layer = container(overlay)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(alignment::Horizontal::Center)
                .align_y(alignment::Vertical::Center);

            stack![base, overlay_layer]
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else {
            base.into()
        }
    } else {
        container(
            text("加载中...")
                .size(16)
                .color(C_TEXT_DIM),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(alignment::Horizontal::Center)
        .align_y(alignment::Vertical::Center)
        .style(|_| container::Style {
            background: Some(Background::Color(C_BG)),
            ..container::Style::default()
        })
        .into()
    };

    let bottom_bar = {
        let close_btn = button(
            text("关闭").size(13).color(C_TEXT),
        )
        .padding([6, 16])
        .on_press(AppMessage::CloseImageViewerWindow)
        .style(|_, _status| button::Style {
            background: Some(Background::Color(Color::from_rgb8(0x2B, 0x31, 0x39))),
            border: border::rounded(6.0).color(C_BORDER).width(1.0),
            text_color: C_TEXT,
            ..button::Style::default()
        });

        container(
            row![
                container(text("")).width(Length::Fill),
                close_btn,
            ]
            .align_y(alignment::Vertical::Center),
        )
        .padding([8, 16])
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(C_TOOLBAR_BG)),
            border: border::width(0.0),
            ..container::Style::default()
        })
    };

    column![toolbar, image_area, bottom_bar]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
