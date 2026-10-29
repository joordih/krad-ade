use gpui_kit::Rgba;
use krad_domain::{Hue, SessionIcon};

use crate::components::IconName;
use crate::theme::{hue_rgb, tinted, Palette};

const WASH_ALPHA: u32 = 0x33;
const ACTIVE_WASH_ALPHA: u32 = 0x59;

pub(super) fn session_wash(hue: Hue, active: bool, pal: &Palette) -> Rgba {
    let alpha = if active {
        ACTIVE_WASH_ALPHA
    } else {
        WASH_ALPHA
    };
    tinted(hue_rgb(hue, pal), alpha)
}

pub(super) const fn session_icon(glyph: SessionIcon) -> IconName {
    match glyph {
        SessionIcon::Folder => IconName::Folder,
        SessionIcon::Terminal => IconName::Terminal,
        SessionIcon::Book => IconName::Book,
        SessionIcon::Note => IconName::Note,
        SessionIcon::Flag => IconName::Flag,
        SessionIcon::Branch => IconName::GitBranch,
        SessionIcon::Layers => IconName::Layers,
        SessionIcon::Home => IconName::Home,
        SessionIcon::Spark => IconName::Magic,
        SessionIcon::Chat => IconName::Chat,
        SessionIcon::Tasks => IconName::Tasks,
        SessionIcon::Diamond => IconName::Diamond,
        SessionIcon::Plug => IconName::Plug,
        SessionIcon::People => IconName::UserGroup,
        SessionIcon::Flow => IconName::Flow,
        SessionIcon::Target => IconName::Target,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_session_icon_draws_its_own_glyph() {
        let glyphs: Vec<IconName> = SessionIcon::ALL
            .iter()
            .map(|&glyph| session_icon(glyph))
            .collect();
        for (index, glyph) in glyphs.iter().enumerate() {
            assert!(
                !glyphs[index + 1..].contains(glyph),
                "{glyph:?} drawn twice"
            );
        }
    }
}
