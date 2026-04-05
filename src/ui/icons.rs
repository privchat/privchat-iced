use iced::widget::{svg, Svg};
use iced::{Color, Length, Theme};

static ICON_SEARCH: &[u8] = br#"<svg viewBox='0 0 24 24' fill='none' xmlns='http://www.w3.org/2000/svg'><path d='M11 18a7 7 0 1 1 0-14 7 7 0 0 1 0 14Z' stroke='currentColor' stroke-width='1.8' stroke-linecap='round' stroke-linejoin='round'/><path d='m20 20-3.5-3.5' stroke='currentColor' stroke-width='1.8' stroke-linecap='round'/></svg>"#;
static ICON_PLUS: &[u8] = br#"<svg viewBox='0 0 24 24' fill='none' xmlns='http://www.w3.org/2000/svg'><path d='M12 5v14M5 12h14' stroke='currentColor' stroke-width='1.9' stroke-linecap='round'/></svg>"#;
static ICON_MESSAGE: &[u8] = br#"<svg viewBox='0 0 24 24' fill='none' xmlns='http://www.w3.org/2000/svg'><path d='M4 5.8C4 4.8 4.8 4 5.8 4h12.4c1 0 1.8.8 1.8 1.8v8.6c0 1-.8 1.8-1.8 1.8H10l-4.6 3.6V16.2H5.8C4.8 16.2 4 15.4 4 14.4V5.8Z' stroke='currentColor' stroke-width='1.7' stroke-linejoin='round'/></svg>"#;
static ICON_CONTACT: &[u8] = br#"<svg viewBox='0 0 24 24' fill='none' xmlns='http://www.w3.org/2000/svg'><circle cx='12' cy='8' r='3.1' stroke='currentColor' stroke-width='1.7'/><path d='M5 19c.7-2.8 3.2-4.5 7-4.5s6.3 1.7 7 4.5' stroke='currentColor' stroke-width='1.7' stroke-linecap='round'/></svg>"#;
static ICON_BOX: &[u8] = br#"<svg viewBox='0 0 24 24' fill='none' xmlns='http://www.w3.org/2000/svg'><path d='m12 3.8 8 4.2v8L12 20.2 4 16V8l8-4.2Z' stroke='currentColor' stroke-width='1.6' stroke-linejoin='round'/><path d='m4 8 8 4.2L20 8' stroke='currentColor' stroke-width='1.6' stroke-linejoin='round'/></svg>"#;
static ICON_COMPASS: &[u8] = br#"<svg viewBox='0 0 24 24' fill='none' xmlns='http://www.w3.org/2000/svg'><circle cx='12' cy='12' r='8' stroke='currentColor' stroke-width='1.6'/><path d='m14.9 9.1-1.8 4.2-4.1 1.8 1.8-4.1 4.1-1.9Z' stroke='currentColor' stroke-width='1.6' stroke-linejoin='round'/></svg>"#;
static ICON_LINK: &[u8] = br#"<svg viewBox='0 0 24 24' fill='none' xmlns='http://www.w3.org/2000/svg'><path d='M10.5 13.5 13.5 10.5' stroke='currentColor' stroke-width='1.8' stroke-linecap='round'/><path d='M9.3 16.5H7.6A3.6 3.6 0 1 1 7.6 9.3h1.8' stroke='currentColor' stroke-width='1.8' stroke-linecap='round'/><path d='M14.7 7.5h1.7a3.6 3.6 0 1 1 0 7.2h-1.8' stroke='currentColor' stroke-width='1.8' stroke-linecap='round'/></svg>"#;
static ICON_MENU: &[u8] = br#"<svg viewBox='0 0 24 24' fill='none' xmlns='http://www.w3.org/2000/svg'><path d='M4.5 7.5h15M4.5 12h15M4.5 16.5h15' stroke='currentColor' stroke-width='1.8' stroke-linecap='round'/></svg>"#;
static ICON_SETTINGS: &[u8] = br#"<svg viewBox='0 0 24 24' fill='none' xmlns='http://www.w3.org/2000/svg'><path d='M12 8.5a3.5 3.5 0 1 0 0 7 3.5 3.5 0 0 0 0-7Z' stroke='currentColor' stroke-width='1.7'/><path d='M19.3 13.2v-2.4l-1.9-.6a5.9 5.9 0 0 0-.6-1.4l.9-1.8-1.7-1.7-1.8.9c-.5-.3-.9-.5-1.4-.6L12 3.7l-2.4 0-.6 1.9c-.5.1-1 .3-1.4.6l-1.8-.9-1.7 1.7.9 1.8c-.3.5-.5.9-.6 1.4l-1.9.6v2.4l1.9.6c.1.5.3 1 .6 1.4l-.9 1.8 1.7 1.7 1.8-.9c.4.3.9.5 1.4.6l.6 1.9h2.4l.6-1.9c.5-.1 1-.3 1.4-.6l1.8.9 1.7-1.7-.9-1.8c.3-.4.5-.9.6-1.4l1.9-.6Z' stroke='currentColor' stroke-width='1.5' stroke-linejoin='round'/></svg>"#;
static ICON_DOTS: &[u8] = br#"<svg viewBox='0 0 24 24' fill='none' xmlns='http://www.w3.org/2000/svg'><circle cx='6' cy='12' r='1.7' fill='currentColor'/><circle cx='12' cy='12' r='1.7' fill='currentColor'/><circle cx='18' cy='12' r='1.7' fill='currentColor'/></svg>"#;
static ICON_PHONE: &[u8] = br#"<svg viewBox='0 0 24 24' fill='none' xmlns='http://www.w3.org/2000/svg'><path d='M7.4 4.8c.4-.8 1.5-1.1 2.2-.6l1.9 1.2c.6.4.9 1.2.6 1.9l-.9 2.1c.7 1.5 1.9 2.7 3.4 3.4l2.1-.9c.7-.3 1.5 0 1.9.6l1.2 1.9c.5.7.2 1.8-.6 2.2l-1.6.9c-.8.4-1.8.5-2.7.2A17 17 0 0 1 5.2 8.2c-.3-.9-.2-1.9.2-2.7l2-1.1Z' stroke='currentColor' stroke-width='1.6' stroke-linejoin='round'/></svg>"#;
static ICON_BUBBLE_OUTLINE: &[u8] = br#"<svg viewBox='0 0 24 24' fill='none' xmlns='http://www.w3.org/2000/svg'><path d='M4.3 5.6c0-1 .8-1.8 1.8-1.8h11.8c1 0 1.8.8 1.8 1.8v8.2c0 1-.8 1.8-1.8 1.8h-6l-4.8 3.9v-3.9H6.1c-1 0-1.8-.8-1.8-1.8V5.6Z' stroke='currentColor' stroke-width='1.6' stroke-linejoin='round'/></svg>"#;
static ICON_CHEVRON_DOWN: &[u8] = br#"<svg viewBox='0 0 24 24' fill='none' xmlns='http://www.w3.org/2000/svg'><path d='m7 10 5 5 5-5' stroke='currentColor' stroke-width='1.9' stroke-linecap='round' stroke-linejoin='round'/></svg>"#;
static ICON_SMILE: &[u8] = br#"<svg viewBox='0 0 24 24' fill='none' xmlns='http://www.w3.org/2000/svg'><circle cx='12' cy='12' r='8.5' stroke='currentColor' stroke-width='1.5'/><path d='M8.6 14.1c.8 1.2 2 1.9 3.4 1.9s2.6-.7 3.4-1.9' stroke='currentColor' stroke-width='1.5' stroke-linecap='round'/><circle cx='9.2' cy='10.2' r='1' fill='currentColor'/><circle cx='14.8' cy='10.2' r='1' fill='currentColor'/></svg>"#;
static ICON_FOLDER: &[u8] = br#"<svg viewBox='0 0 24 24' fill='none' xmlns='http://www.w3.org/2000/svg'><path d='M3.8 8.3c0-1 .8-1.8 1.8-1.8h4.1l1.4 1.5h7.3c1 0 1.8.8 1.8 1.8v7.4c0 1-.8 1.8-1.8 1.8H5.6c-1 0-1.8-.8-1.8-1.8V8.3Z' stroke='currentColor' stroke-width='1.6' stroke-linejoin='round'/></svg>"#;
static ICON_SCISSORS: &[u8] = br#"<svg viewBox='0 0 24 24' fill='none' xmlns='http://www.w3.org/2000/svg'><circle cx='6.6' cy='7.2' r='2.1' stroke='currentColor' stroke-width='1.5'/><circle cx='6.6' cy='16.8' r='2.1' stroke='currentColor' stroke-width='1.5'/><path d='M20 5.8 9 12l11 6.2M11 10.9l-1.8-1' stroke='currentColor' stroke-width='1.5' stroke-linecap='round' stroke-linejoin='round'/></svg>"#;
static ICON_IMAGE: &[u8] = br#"<svg viewBox='0 0 24 24' fill='none' xmlns='http://www.w3.org/2000/svg'><rect x='4' y='5' width='16' height='14' rx='1.8' stroke='currentColor' stroke-width='1.6'/><path d='m7.5 15 2.8-2.8 2.2 2.2 2.5-2.5L18 15' stroke='currentColor' stroke-width='1.6' stroke-linecap='round' stroke-linejoin='round'/><circle cx='9' cy='9' r='1.1' fill='currentColor'/></svg>"#;

#[derive(Debug, Clone, Copy)]
pub enum Icon {
    Search,
    Plus,
    Message,
    Contact,
    Box,
    Compass,
    Link,
    Menu,
    Settings,
    Dots,
    Phone,
    BubbleOutline,
    ChevronDown,
    Smile,
    Folder,
    Scissors,
    Image,
}

pub fn render(icon: Icon, size: f32, color: Color) -> Svg<'static, Theme> {
    let bytes = match icon {
        Icon::Search => ICON_SEARCH,
        Icon::Plus => ICON_PLUS,
        Icon::Message => ICON_MESSAGE,
        Icon::Contact => ICON_CONTACT,
        Icon::Box => ICON_BOX,
        Icon::Compass => ICON_COMPASS,
        Icon::Link => ICON_LINK,
        Icon::Menu => ICON_MENU,
        Icon::Settings => ICON_SETTINGS,
        Icon::Dots => ICON_DOTS,
        Icon::Phone => ICON_PHONE,
        Icon::BubbleOutline => ICON_BUBBLE_OUTLINE,
        Icon::ChevronDown => ICON_CHEVRON_DOWN,
        Icon::Smile => ICON_SMILE,
        Icon::Folder => ICON_FOLDER,
        Icon::Scissors => ICON_SCISSORS,
        Icon::Image => ICON_IMAGE,
    };

    svg(svg::Handle::from_memory(bytes))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(move |_theme: &Theme, _status| svg::Style { color: Some(color) })
}
