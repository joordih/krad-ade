use krad_domain::SessionIcon;

use crate::components::IconName;

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
