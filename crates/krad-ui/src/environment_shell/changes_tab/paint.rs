use gpui_kit::component::ActiveTheme;
use gpui_kit::{AppContext as _, Context};
use krad_domain::{EnvironmentName, TerminalId};

use super::super::review_view::{highlight_diff, LineHighlights};
use super::super::EnvironmentShell;
use super::DiffLoad;
use crate::language::editor_id;

pub(in crate::environment_shell) enum DiffColors {
    Unpainted,
    Painting(String),
    Painted {
        theme: String,
        lines: Vec<LineHighlights>,
    },
}

impl DiffColors {
    pub(in crate::environment_shell) fn stale_for(&self, theme: &str) -> bool {
        match self {
            Self::Unpainted => true,
            Self::Painting(painted) | Self::Painted { theme: painted, .. } => painted != theme,
        }
    }

    pub(in crate::environment_shell) fn painting(&self, theme: &str) -> bool {
        matches!(self, Self::Painting(painted) if painted == theme)
    }

    pub(in crate::environment_shell) fn line(&self, index: usize) -> Option<&LineHighlights> {
        match self {
            Self::Painted { lines, .. } => lines.get(index).filter(|line| !line.is_empty()),
            Self::Unpainted | Self::Painting(_) => None,
        }
    }
}

impl EnvironmentShell {
    pub(super) fn paint_diff(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) {
        let theme = &cx.theme().highlight_theme;
        let Some(path) = self
            .envs
            .get(env)
            .and_then(|live| live.diffs.get(&id))
            .filter(|diff| match &diff.load {
                DiffLoad::Ready(read) => read.colors.stale_for(&theme.name),
                DiffLoad::Pending | DiffLoad::Reading | DiffLoad::Failed(_) => false,
            })
            .map(|diff| diff.path.clone())
        else {
            return;
        };
        let theme = theme.clone();
        let grammar = self
            .language_catalog()
            .for_path(&path)
            .and_then(editor_id)
            .map(str::to_owned);
        let Some(DiffLoad::Ready(read)) = self
            .envs
            .get_mut(env)
            .and_then(|live| live.diffs.get_mut(&id))
            .map(|diff| &mut diff.load)
        else {
            return;
        };
        let Some(grammar) = grammar else {
            read.colors = DiffColors::Painted {
                theme: theme.name.clone(),
                lines: Vec::new(),
            };
            return;
        };
        read.colors = DiffColors::Painting(theme.name.clone());
        let lines = read.lines.clone();
        let painting = cx.background_spawn(async move {
            let lit = highlight_diff(&lines, &grammar, &theme);
            (lines, lit, theme)
        });
        let env = env.clone();
        cx.spawn(async move |this, cx| {
            let (lines, lit, theme) = painting.await;
            this.update(cx, |shell, cx| {
                let Some(DiffLoad::Ready(read)) = shell
                    .envs
                    .get_mut(&env)
                    .and_then(|live| live.diffs.get_mut(&id))
                    .map(|diff| &mut diff.load)
                else {
                    return;
                };
                if read.colors.painting(&theme.name) && read.lines == lines {
                    read.colors = DiffColors::Painted {
                        theme: theme.name.clone(),
                        lines: lit,
                    };
                    cx.notify();
                }
            })
            .unwrap_or_default();
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DARK: &str = "krad-editor-dracula-dark";
    const LIGHT: &str = "krad-editor-dracula-light";

    #[test]
    fn colors_are_painted_once_per_theme_and_again_when_it_changes() {
        assert!(DiffColors::Unpainted.stale_for(DARK));
        let painting = DiffColors::Painting(DARK.to_owned());
        assert!(!painting.stale_for(DARK));
        assert!(painting.painting(DARK) && !painting.painting(LIGHT));
        assert_eq!(painting.line(0), None);
        let painted = DiffColors::Painted {
            theme: DARK.to_owned(),
            lines: vec![LineHighlights::new()],
        };
        assert!(!painted.stale_for(DARK));
        assert!(painted.stale_for(LIGHT));
        assert_eq!(
            painted.line(0),
            None,
            "a plain line keeps the row's own color"
        );
    }
}
