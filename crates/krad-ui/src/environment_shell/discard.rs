use gpui_kit::{
    div, px, rgb, AnyElement, AppContext as _, Context, FontWeight, ParentElement, Styled, Window,
};
use krad_domain::TerminalId;
use krad_repo::{discard_change, read_text, ChangeKind, FileChange, RepoError};
use std::path::PathBuf;

use super::changes_tab::reverted_block;
use super::dialog_parts::{sheet_footer, sheet_overlay, sheet_panel, sheet_title};
use super::native_alert::Confirmation;
use super::EnvironmentShell;
use crate::components::{Button, ButtonVariant};
use crate::theme::{palette, LINE_BASE, LINE_SM, SPACE_2, SPACE_5, SPACE_6, TEXT_BASE, TEXT_SM};

const DISCARD_TITLE: &str = "Discard changes?";
const DELETE_TITLE: &str = "Delete file?";
const REVERT_TITLE: &str = "Revert this block?";
const DISCARD_TEXT: &str = "Its changes are lost. This cannot be undone.";
const DELETE_TEXT: &str = "The file is not tracked, so it is deleted. This cannot be undone.";
const REVERT_TEXT: &str = "The block goes back to the last commit. This cannot be undone.";
const DISCARD_CONFIRM: &str = "Discard";
const DELETE_CONFIRM: &str = "Delete";
const REVERT_CONFIRM: &str = "Revert";
const CANCEL: &str = "Cancel";
const STALE_TOAST: &str = "The file changed since its diff was read; try again.";
const BTN_CLOSE: &str = "discard-close";
const BTN_CANCEL: &str = "discard-cancel";
const BTN_CONFIRM: &str = "discard-confirm";
const DIALOG_W: f32 = 420.0;
const MAX_REVERT_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Discard {
    File { repo: PathBuf, change: FileChange },
    Block { tab: TerminalId, start: usize },
}

struct DiscardWords {
    title: &'static str,
    text: &'static str,
    confirm: &'static str,
}

impl Discard {
    fn words(&self) -> DiscardWords {
        match self {
            Self::File { change, .. } if change.kind == ChangeKind::Untracked => DiscardWords {
                title: DELETE_TITLE,
                text: DELETE_TEXT,
                confirm: DELETE_CONFIRM,
            },
            Self::File { .. } => DiscardWords {
                title: DISCARD_TITLE,
                text: DISCARD_TEXT,
                confirm: DISCARD_CONFIRM,
            },
            Self::Block { .. } => DiscardWords {
                title: REVERT_TITLE,
                text: REVERT_TEXT,
                confirm: REVERT_CONFIRM,
            },
        }
    }
}

fn cancel(this: &mut EnvironmentShell, _: &mut Window, cx: &mut Context<EnvironmentShell>) {
    this.pending_discard = None;
    cx.notify();
}

impl EnvironmentShell {
    fn discarded_path(&self, discard: &Discard) -> Option<PathBuf> {
        match discard {
            Discard::File { change, .. } => Some(change.path.clone()),
            Discard::Block { tab, .. } => {
                let live = self.envs.get(&self.active)?;
                let path = live.diff_path(*tab)?;
                Some(self.relative_to_root(path).to_path_buf())
            }
        }
    }

    pub(super) fn ask_discard(
        &mut self,
        discard: Discard,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self.discarded_path(&discard) else {
            return;
        };
        let words = discard.words();
        let detail = format!("{}\n\n{}", path.display(), words.text);
        let confirmation = Confirmation {
            title: words.title,
            detail: &detail,
            confirm: words.confirm,
            cancel: CANCEL,
        };
        let asked_for = discard.clone();
        let asked = Self::confirm_natively(&confirmation, window, cx, |this, confirmed, _, cx| {
            if confirmed {
                this.run_discard(asked_for, cx);
            }
        });
        if !asked {
            self.pending_discard = Some(discard);
            cx.notify();
        }
    }

    fn confirm_discard(&mut self, cx: &mut Context<Self>) {
        if let Some(discard) = self.pending_discard.take() {
            self.run_discard(discard, cx);
        }
    }

    fn run_discard(&mut self, discard: Discard, cx: &mut Context<Self>) {
        let link = self.active_link();
        let task =
            match discard {
                Discard::File { repo, change } => cx.background_spawn(async move {
                    discard_change(&link, &repo, &change).map(|()| true)
                }),
                Discard::Block { tab, start } => {
                    let Some((path, lines)) = self
                        .envs
                        .get(&self.active)
                        .and_then(|live| live.diff_lines(tab))
                    else {
                        return;
                    };
                    cx.background_spawn(async move {
                        let current = read_text(&link, &path, MAX_REVERT_BYTES)?;
                        let Some(text) = reverted_block(&lines, start, &current.text)
                            .filter(|_| !current.truncated)
                        else {
                            return Ok(false);
                        };
                        link.write(&path, text.as_bytes())
                            .map_err(RepoError::from)
                            .map(|()| true)
                    })
                }
            };
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |shell, cx| {
                match result {
                    Ok(true) => {}
                    Ok(false) => shell.toast(STALE_TOAST, cx),
                    Err(error) => shell.toast_failure(&error),
                }
                shell.refresh_git(cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn discard_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let discard = self.pending_discard.as_ref()?;
        let shown = self.discarded_path(discard)?.display().to_string();
        let words = discard.words();
        let pal = palette(cx);
        let sheet = sheet_panel(DIALOG_W, pal)
            .child(sheet_title(words.title, None, BTN_CLOSE, cancel, cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(SPACE_2))
                    .px(px(SPACE_6))
                    .pb(px(SPACE_5))
                    .child(
                        div()
                            .text_size(px(TEXT_BASE))
                            .line_height(px(LINE_BASE))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(pal.text_strong))
                            .child(shown),
                    )
                    .child(
                        div()
                            .text_size(px(TEXT_SM))
                            .line_height(px(LINE_SM))
                            .text_color(rgb(pal.text_muted))
                            .child(words.text),
                    ),
            )
            .child(
                sheet_footer(pal)
                    .justify_end()
                    .child(
                        Button::new(BTN_CANCEL)
                            .label(CANCEL)
                            .variant(ButtonVariant::Secondary)
                            .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                                cancel(this, window, cx);
                            })),
                    )
                    .child(
                        Button::new(BTN_CONFIRM)
                            .label(words.confirm)
                            .variant(ButtonVariant::Destructive)
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.confirm_discard(cx);
                            })),
                    ),
            );
        Some(sheet_overlay(sheet, cancel, cx))
    }
}
