use krad_domain::{Theme, ThemeColor, ANSI_COLORS};

const ANSI_NAMES: [&str; ANSI_COLORS] = [
    "Black",
    "Red",
    "Green",
    "Yellow",
    "Blue",
    "Magenta",
    "Cyan",
    "White",
    "Bright black",
    "Bright red",
    "Bright green",
    "Bright yellow",
    "Bright blue",
    "Bright magenta",
    "Bright cyan",
    "Bright white",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ColorGroup {
    Interface,
    Terminal,
    Syntax,
}

impl ColorGroup {
    pub(super) const ALL: [Self; 3] = [Self::Interface, Self::Terminal, Self::Syntax];

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Interface => "Interface",
            Self::Terminal => "Terminal",
            Self::Syntax => "Syntax",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum ColorSlot {
    Background,
    Surface,
    Panel,
    Text,
    MutedText,
    Accent,
    OnAccent,
    Success,
    Warning,
    Error,
    Info,
    TerminalBackground,
    TerminalForeground,
    Cursor,
    Selection,
    Ansi(usize),
    Comment,
    Keyword,
    Str,
    Escape,
    Number,
    Constant,
    Attribute,
    TypeName,
    Operator,
    Function,
    Property,
    Tag,
    Variable,
    Punctuation,
}

impl ColorSlot {
    const FIXED: [Self; 29] = [
        Self::Background,
        Self::Surface,
        Self::Panel,
        Self::Text,
        Self::MutedText,
        Self::Accent,
        Self::OnAccent,
        Self::Success,
        Self::Warning,
        Self::Error,
        Self::Info,
        Self::TerminalBackground,
        Self::TerminalForeground,
        Self::Cursor,
        Self::Selection,
        Self::Comment,
        Self::Keyword,
        Self::Str,
        Self::Escape,
        Self::Number,
        Self::Constant,
        Self::Attribute,
        Self::TypeName,
        Self::Operator,
        Self::Function,
        Self::Property,
        Self::Tag,
        Self::Variable,
        Self::Punctuation,
    ];

    pub(super) fn all() -> Vec<Self> {
        let (head, tail) = Self::FIXED.split_at(15);
        head.iter()
            .copied()
            .chain((0..ANSI_COLORS).map(Self::Ansi))
            .chain(tail.iter().copied())
            .collect()
    }

    pub(super) const fn group(self) -> ColorGroup {
        match self {
            Self::Background
            | Self::Surface
            | Self::Panel
            | Self::Text
            | Self::MutedText
            | Self::Accent
            | Self::OnAccent
            | Self::Success
            | Self::Warning
            | Self::Error
            | Self::Info => ColorGroup::Interface,
            Self::TerminalBackground
            | Self::TerminalForeground
            | Self::Cursor
            | Self::Selection
            | Self::Ansi(_) => ColorGroup::Terminal,
            _ => ColorGroup::Syntax,
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Background | Self::TerminalBackground => "Background",
            Self::Surface => "Surface",
            Self::Panel => "Panel",
            Self::Text => "Text",
            Self::MutedText => "Muted text",
            Self::Accent => "Accent",
            Self::OnAccent => "Text on accent",
            Self::Success => "Success",
            Self::Warning => "Warning",
            Self::Error => "Error",
            Self::Info => "Info",
            Self::TerminalForeground => "Foreground",
            Self::Cursor => "Cursor",
            Self::Selection => "Selection",
            Self::Ansi(index) => ANSI_NAMES.get(index).copied().unwrap_or_default(),
            Self::Comment => "Comment",
            Self::Keyword => "Keyword",
            Self::Str => "String",
            Self::Escape => "Escape",
            Self::Number => "Number",
            Self::Constant => "Constant",
            Self::Attribute => "Attribute",
            Self::TypeName => "Type",
            Self::Operator => "Operator",
            Self::Function => "Function",
            Self::Property => "Property",
            Self::Tag => "Tag",
            Self::Variable => "Variable",
            Self::Punctuation => "Punctuation",
        }
    }

    fn color_mut(self, theme: &mut Theme) -> Option<&mut ThemeColor> {
        let ui = &mut theme.interface;
        let term = &mut theme.terminal;
        let code = &mut theme.syntax;
        Some(match self {
            Self::Background => &mut ui.background,
            Self::Surface => &mut ui.surface,
            Self::Panel => &mut ui.panel,
            Self::Text => &mut ui.text,
            Self::MutedText => &mut ui.muted_text,
            Self::Accent => &mut ui.accent,
            Self::OnAccent => &mut ui.on_accent,
            Self::Success => &mut ui.success,
            Self::Warning => &mut ui.warning,
            Self::Error => &mut ui.error,
            Self::Info => &mut ui.info,
            Self::TerminalBackground => &mut term.background,
            Self::TerminalForeground => &mut term.foreground,
            Self::Cursor => &mut term.cursor,
            Self::Selection => &mut term.selection,
            Self::Ansi(index) => term.ansi.get_mut(index)?,
            Self::Comment => &mut code.comment,
            Self::Keyword => &mut code.keyword,
            Self::Str => &mut code.string,
            Self::Escape => &mut code.escape,
            Self::Number => &mut code.number,
            Self::Constant => &mut code.constant,
            Self::Attribute => &mut code.attribute,
            Self::TypeName => &mut code.type_name,
            Self::Operator => &mut code.operator,
            Self::Function => &mut code.function,
            Self::Property => &mut code.property,
            Self::Tag => &mut code.tag,
            Self::Variable => &mut code.variable,
            Self::Punctuation => &mut code.punctuation,
        })
    }

    pub(super) fn get(self, theme: &Theme) -> Option<ThemeColor> {
        let (ui, term, code) = (&theme.interface, &theme.terminal, &theme.syntax);
        Some(match self {
            Self::Background => ui.background,
            Self::Surface => ui.surface,
            Self::Panel => ui.panel,
            Self::Text => ui.text,
            Self::MutedText => ui.muted_text,
            Self::Accent => ui.accent,
            Self::OnAccent => ui.on_accent,
            Self::Success => ui.success,
            Self::Warning => ui.warning,
            Self::Error => ui.error,
            Self::Info => ui.info,
            Self::TerminalBackground => term.background,
            Self::TerminalForeground => term.foreground,
            Self::Cursor => term.cursor,
            Self::Selection => term.selection,
            Self::Comment => code.comment,
            Self::Keyword => code.keyword,
            Self::Str => code.string,
            Self::Escape => code.escape,
            Self::Number => code.number,
            Self::Constant => code.constant,
            Self::Attribute => code.attribute,
            Self::TypeName => code.type_name,
            Self::Operator => code.operator,
            Self::Function => code.function,
            Self::Property => code.property,
            Self::Tag => code.tag,
            Self::Variable => code.variable,
            Self::Punctuation => code.punctuation,
            Self::Ansi(index) => *term.ansi.get(index)?,
        })
    }

    pub(super) fn set(self, theme: &mut Theme, color: ThemeColor) {
        if let Some(slot) = self.color_mut(theme) {
            *slot = color;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_slot_reads_back_what_was_written_and_belongs_to_one_group() {
        let mut theme: Theme = include_str!("../../../../../../assets/themes/library/nord.json")
            .parse()
            .unwrap_or_else(|_| unreachable!());
        let slots = ColorSlot::all();
        assert_eq!(slots.len(), 29 + ANSI_COLORS);
        for (index, slot) in slots.iter().enumerate() {
            let color = ThemeColor(u32::try_from(index).unwrap_or_default());
            slot.set(&mut theme, color);
            assert_eq!(slot.get(&theme), Some(color), "{slot:?}");
        }
        let terminal = slots
            .iter()
            .filter(|slot| slot.group() == ColorGroup::Terminal)
            .count();
        assert_eq!(terminal, 4 + ANSI_COLORS);
    }
}
