use gpui_kit::{px, rgb, Div, FontWeight, Hsla, Styled};
use krad_extensions::api::view::{
    Align, ColorToken, Direction, Justify, Radius, Size, Space, Style, TextSize, Tone, Weight,
    Wrap, MAX_FIXED_SIZE,
};
use krad_extensions::api::Icon;

use crate::components::IconName;
use crate::theme::{
    Palette, LINE_BASE, LINE_LG, LINE_SM, LINE_XS, RADIUS_LG, RADIUS_MD, RADIUS_SM, SPACE_1,
    SPACE_2, SPACE_3, SPACE_5, SPACE_6, TEXT_BASE, TEXT_LG, TEXT_SM, TEXT_XS,
};

const ROUND_FULL: f32 = 9999.0;

pub(crate) const fn space(space: Space) -> f32 {
    match space {
        Space::None => 0.0,
        Space::Xs => SPACE_1,
        Space::Sm => SPACE_2,
        Space::Md => SPACE_3,
        Space::Lg => SPACE_5,
        Space::Xl => SPACE_6,
    }
}

pub(crate) const fn radius(radius: Radius) -> f32 {
    match radius {
        Radius::None => 0.0,
        Radius::Sm => RADIUS_SM,
        Radius::Md => RADIUS_MD,
        Radius::Lg => RADIUS_LG,
        Radius::Full => ROUND_FULL,
    }
}

pub(crate) const fn text_size(size: TextSize) -> (f32, f32) {
    match size {
        TextSize::Xs => (TEXT_XS, LINE_XS),
        TextSize::Sm => (TEXT_SM, LINE_SM),
        TextSize::Md => (TEXT_BASE, LINE_BASE),
        TextSize::Lg => (TEXT_LG, LINE_LG),
    }
}

pub(crate) const fn weight(weight: Weight) -> FontWeight {
    match weight {
        Weight::Regular => FontWeight::NORMAL,
        Weight::Medium => FontWeight::MEDIUM,
        Weight::Bold => FontWeight::BOLD,
    }
}

pub(crate) fn color(token: ColorToken, pal: &Palette) -> Hsla {
    let solid = |value: u32| -> Hsla { rgb(value).into() };
    match token {
        ColorToken::TextStrong => solid(pal.text_strong),
        ColorToken::TextDefault => solid(pal.text_default),
        ColorToken::TextSubtle => solid(pal.text_subtle),
        ColorToken::TextMuted => solid(pal.text_muted),
        ColorToken::TextDisabled => solid(pal.text_disabled),
        ColorToken::Accent => solid(pal.accent),
        ColorToken::OnAccent => solid(pal.on_accent),
        ColorToken::Success => solid(pal.success_fg),
        ColorToken::Warning => solid(pal.warning_fg),
        ColorToken::Error => solid(pal.error_fg),
        ColorToken::Info => solid(pal.info_fg),
        ColorToken::Surface => solid(pal.bg_surface),
        ColorToken::Panel => solid(pal.bg_panel),
        ColorToken::Hover => pal.bg_hover.into(),
        ColorToken::Selected => pal.bg_selected.into(),
        ColorToken::Border => pal.border.into(),
        ColorToken::Divider => pal.divider.into(),
        ColorToken::Chip => solid(pal.chip_bg),
        ColorToken::ChipText => solid(pal.chip_fg),
    }
}

pub(crate) const fn tone_color(tone: Tone, pal: &Palette) -> u32 {
    match tone {
        Tone::Neutral => pal.text_default,
        Tone::Info => pal.info_fg,
        Tone::Success => pal.success_fg,
        Tone::Warning => pal.warning_fg,
        Tone::Error => pal.error_fg,
    }
}

pub(crate) const fn icon_name(icon: Icon) -> IconName {
    match icon {
        Icon::Add => IconName::Add,
        Icon::Attach => IconName::Attach,
        Icon::Bell => IconName::Bell,
        Icon::Book => IconName::Book,
        Icon::Cancel => IconName::CancelCircle,
        Icon::Check => IconName::CheckCircle,
        Icon::ChevronDown => IconName::ChevronDown,
        Icon::ChevronLeft => IconName::ChevronLeft,
        Icon::ChevronRight => IconName::ChevronRight,
        Icon::ChevronUp => IconName::ChevronUp,
        Icon::Close => IconName::Close,
        Icon::Copy => IconName::Copy,
        Icon::Exchange => IconName::Exchange,
        Icon::Expand => IconName::Expand,
        Icon::File => IconName::File,
        Icon::Flag => IconName::Flag,
        Icon::Flash => IconName::Flash,
        Icon::Flow => IconName::Flow,
        Icon::Folder => IconName::Folder,
        Icon::GitBranch => IconName::GitBranch,
        Icon::Key => IconName::Key,
        Icon::Layers => IconName::Layers,
        Icon::ListView => IconName::ListView,
        Icon::Loading => IconName::Loading,
        Icon::Lock => IconName::Lock,
        Icon::Magic => IconName::Magic,
        Icon::Minus => IconName::Minus,
        Icon::More => IconName::More,
        Icon::Note => IconName::Note,
        Icon::OpenExternal => IconName::OpenExternal,
        Icon::Pencil => IconName::Pencil,
        Icon::Play => IconName::Play,
        Icon::Plug => IconName::Plug,
        Icon::Refresh => IconName::Refresh,
        Icon::Repeat => IconName::Repeat,
        Icon::Search => IconName::Search,
        Icon::Settings => IconName::Settings,
        Icon::Stop => IconName::Stop,
        Icon::Target => IconName::Target,
        Icon::Tasks => IconName::Tasks,
        Icon::Terminal => IconName::Terminal,
        Icon::TreeView => IconName::TreeView,
        Icon::Undo => IconName::Undo,
        Icon::Workflow => IconName::Workflow,
    }
}

pub(crate) fn fixed(size: Size) -> Option<f32> {
    match size.bounded() {
        Size::Fixed(pixels) => Some(f32::from(pixels.min(MAX_FIXED_SIZE))),
        Size::Auto | Size::Fill => None,
    }
}

fn sized(base: Div, width: Size, height: Size) -> Div {
    let base = match (width, fixed(width)) {
        (_, Some(pixels)) => base.w(px(pixels)),
        (Size::Fill, None) => base.w_full(),
        _ => base,
    };
    match (height, fixed(height)) {
        (_, Some(pixels)) => base.h(px(pixels)),
        (Size::Fill, None) => base.h_full().flex_1(),
        _ => base,
    }
}

pub(crate) fn styled(base: Div, style: &Style, pal: &Palette) -> Div {
    let mut base = sized(base, style.width, style.height)
        .p(px(space(style.padding)))
        .gap(px(space(style.gap)))
        .rounded(px(radius(style.radius)));
    if let Some(background) = style.background {
        base = base.bg(color(background, pal));
    }
    if let Some(border) = style.border {
        base = base.border_1().border_color(color(border, pal));
    }
    base
}

pub(crate) fn sized_box(base: Div, width: Size, height: Size) -> Div {
    sized(base, width, height)
}

pub(crate) fn flex_layout(
    base: Div,
    direction: Direction,
    align: Align,
    justify: Justify,
    wrap: Wrap,
) -> Div {
    let base = match direction {
        Direction::Row => base.flex().flex_row(),
        Direction::Column => base.flex().flex_col(),
    };
    let base = match align {
        Align::Start => base.items_start(),
        Align::Center => base.items_center(),
        Align::End => base.items_end(),
        Align::Stretch => base,
    };
    let base = match justify {
        Justify::Start => base.justify_start(),
        Justify::Center => base.justify_center(),
        Justify::End => base.justify_end(),
        Justify::Between => base.justify_between(),
    };
    match wrap {
        Wrap::Wrap => base.flex_wrap(),
        Wrap::NoWrap => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spaces_follow_the_theme_scale() {
        assert!(space(Space::None).abs() < f32::EPSILON);
        assert!((space(Space::Sm) - SPACE_2).abs() < f32::EPSILON);
        assert!(space(Space::Xl) > space(Space::Lg));
    }

    #[test]
    fn fixed_sizes_are_capped_and_others_have_no_pixels() {
        assert_eq!(
            fixed(Size::Fixed(u16::MAX)),
            Some(f32::from(MAX_FIXED_SIZE))
        );
        assert_eq!(fixed(Size::Fixed(40)), Some(40.0));
        assert_eq!(fixed(Size::Fill), None);
        assert_eq!(fixed(Size::Auto), None);
    }

    #[test]
    fn status_tokens_and_tones_use_the_status_colors() {
        let pal = crate::theme::Appearance::default().palette();
        assert_eq!(color(ColorToken::Error, pal), rgb(pal.error_fg).into());
        assert_eq!(tone_color(Tone::Warning, pal), pal.warning_fg);
        assert_eq!(tone_color(Tone::Neutral, pal), pal.text_default);
    }

    #[test]
    fn every_api_icon_has_an_app_icon() {
        assert_eq!(icon_name(Icon::Layers), IconName::Layers);
        assert_eq!(icon_name(Icon::Check), IconName::CheckCircle);
    }
}
