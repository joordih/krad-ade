use gpui_kit::component::ActiveTheme;
use gpui_kit::{AppContext as _, Context};
use krad_domain::review::MergeRequestIid;
use std::path::Path;

use super::Remote;
use crate::environment_shell::changes_tab::DiffColors;
use crate::environment_shell::EnvironmentShell;
use crate::language::editor_id;

impl EnvironmentShell {
    pub(super) fn paint_review_file(&mut self, cx: &mut Context<Self>) {
        let theme = cx.theme().highlight_theme.clone();
        let Some(detail) = self.review.detail.as_ref() else {
            return;
        };
        let Some((index, file)) = detail.shown_file() else {
            return;
        };
        let stale = detail
            .colors
            .get(&index)
            .is_none_or(|colors| colors.stale_for(&theme.name));
        if !stale {
            return;
        }
        let grammar = self
            .language_catalog()
            .for_path(Path::new(&file.change.new_path))
            .and_then(editor_id)
            .map(str::to_owned);
        let lines = file.lines.clone();
        let iid = detail.request.iid;
        let Some(detail) = self.review.detail.as_mut() else {
            return;
        };
        let Some(grammar) = grammar else {
            detail.colors.insert(
                index,
                DiffColors::Painted {
                    theme: theme.name.clone(),
                    lines: Vec::new(),
                },
            );
            return;
        };
        detail
            .colors
            .insert(index, DiffColors::Painting(theme.name.clone()));
        let painting = cx.background_spawn(async move {
            let lit = super::highlight_diff(&lines, &grammar, &theme);
            (lines, lit, theme.name.clone())
        });
        cx.spawn(async move |this, cx| {
            let (lines, lit, theme) = painting.await;
            this.update(cx, |shell, cx| {
                shell.painted_review_file(iid, index, &lines, lit, theme);
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn painted_review_file(
        &mut self,
        iid: MergeRequestIid,
        index: usize,
        lines: &[super::DiffLine],
        lit: Vec<super::LineHighlights>,
        theme: String,
    ) {
        let Some(detail) = self
            .review
            .detail
            .as_mut()
            .filter(|detail| detail.request.iid == iid)
        else {
            return;
        };
        let Remote::Ready(files) = &detail.files else {
            return;
        };
        let current = files.get(index).is_some_and(|file| file.lines == lines);
        let waiting = detail
            .colors
            .get(&index)
            .is_some_and(|colors| colors.painting(&theme));
        if current && waiting {
            detail
                .colors
                .insert(index, DiffColors::Painted { theme, lines: lit });
        }
    }
}
