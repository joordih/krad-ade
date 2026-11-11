use gpui_kit::{div, px, rgb, AnyElement, Context, IntoElement, ParentElement, Styled};
use krad_extensions::api::protocol::{ProgressParams, ProgressState, MAX_PERCENT};
use krad_extensions::api::ProgressToken;

use super::super::EnvironmentShell;
use super::SlotKey;
use crate::components::floating::floating_surface;
use crate::components::working_dots;
use crate::theme::{
    palette, LINE_SM, LINE_XS, RADIUS_XS, SPACE_1, SPACE_2, SPACE_3, TEXT_SM, TEXT_XS, WINDOW_INSET,
};

pub(super) const MAX_PROGRESS: usize = 6;
const PROGRESS_WIDTH: f32 = 300.0;
const PROGRESS_BOTTOM: f32 = WINDOW_INSET + SPACE_3;
const PROGRESS_RIGHT: f32 = WINDOW_INSET + SPACE_3;
const BAR_HEIGHT: f32 = 3.0;
const TITLE_SEPARATOR: &str = ": ";
const PERCENT_SUFFIX: &str = "%";
const DOTS_KEY_BASE: u64 = 0x7072_6f67;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProgressItem {
    key: SlotKey,
    token: ProgressToken,
    title: String,
    message: Option<String>,
    percent: Option<u8>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ProgressBoard {
    items: Vec<ProgressItem>,
}

impl ProgressBoard {
    pub(super) fn apply(&mut self, key: &SlotKey, extension: &str, params: ProgressParams) {
        let position = self
            .items
            .iter()
            .position(|item| item.key == *key && item.token == params.token);
        match params.state.bounded() {
            ProgressState::Begin { title } => {
                if let Some(at) = position {
                    self.items.remove(at);
                }
                self.items.push(ProgressItem {
                    key: key.clone(),
                    token: params.token,
                    title: format!("{extension}{TITLE_SEPARATOR}{title}"),
                    message: None,
                    percent: None,
                });
                let excess = self.items.len().saturating_sub(MAX_PROGRESS);
                self.items.drain(..excess);
            }
            ProgressState::Report { message, percent } => {
                let Some(item) = position.and_then(|at| self.items.get_mut(at)) else {
                    return;
                };
                if message.is_some() {
                    item.message = message;
                }
                if percent.is_some() {
                    item.percent = percent;
                }
            }
            ProgressState::End => {
                if let Some(at) = position {
                    self.items.remove(at);
                }
            }
        }
    }

    pub(super) fn forget(&mut self, key: &SlotKey) {
        self.items.retain(|item| item.key != *key);
    }

    fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl EnvironmentShell {
    pub(super) fn track_progress(&mut self, key: &SlotKey, title: &str, params: ProgressParams) {
        if let ProgressState::Begin { title: what } = &params.state {
            self.activity_log
                .record(format!("{title}{TITLE_SEPARATOR}{what}"));
        }
        self.extensions.progress.apply(key, title, params);
    }

    pub(in crate::environment_shell) fn extension_progress(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.extensions.progress.is_empty() {
            return None;
        }
        let pal = palette(cx);
        let rows = self
            .extensions
            .progress
            .items
            .iter()
            .zip(DOTS_KEY_BASE..)
            .map(|(item, dots)| {
                let detail = match (&item.message, item.percent) {
                    (Some(message), Some(percent)) => {
                        Some(format!("{message} · {percent}{PERCENT_SUFFIX}"))
                    }
                    (Some(message), None) => Some(message.clone()),
                    (None, Some(percent)) => Some(format!("{percent}{PERCENT_SUFFIX}")),
                    (None, None) => None,
                };
                let bar = item.percent.map(|percent| {
                    let share = f32::from(percent) / f32::from(MAX_PERCENT);
                    div()
                        .h(px(BAR_HEIGHT))
                        .w_full()
                        .rounded(px(RADIUS_XS))
                        .bg(pal.bg_hover)
                        .child(
                            div()
                                .h_full()
                                .w(gpui_kit::relative(share))
                                .rounded(px(RADIUS_XS))
                                .bg(rgb(pal.accent)),
                        )
                });
                div()
                    .flex()
                    .flex_col()
                    .gap(px(SPACE_1))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(SPACE_2))
                            .child(working_dots(dots, pal.text_muted))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(px(TEXT_SM))
                                    .line_height(px(LINE_SM))
                                    .text_color(rgb(pal.text_default))
                                    .child(item.title.clone()),
                            ),
                    )
                    .children(detail.map(|detail| {
                        div()
                            .text_size(px(TEXT_XS))
                            .line_height(px(LINE_XS))
                            .text_color(rgb(pal.text_muted))
                            .child(detail)
                    }))
                    .children(bar)
            });
        Some(
            floating_surface(pal, cx)
                .absolute()
                .bottom(px(PROGRESS_BOTTOM))
                .right(px(PROGRESS_RIGHT))
                .w(px(PROGRESS_WIDTH))
                .gap(px(SPACE_2))
                .p(px(SPACE_3))
                .children(rows)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn key() -> Result<SlotKey, krad_extensions::api::IdError> {
        Ok(SlotKey {
            extension: "docker".parse()?,
            environment: None,
        })
    }

    fn params(
        token: &str,
        state: ProgressState,
    ) -> Result<ProgressParams, krad_extensions::api::IdError> {
        Ok(ProgressParams {
            token: token.parse()?,
            state,
        })
    }

    #[test]
    fn a_report_updates_its_begun_item_and_end_removes_it() -> TestResult {
        let mut board = ProgressBoard::default();
        let key = key()?;
        board.apply(
            &key,
            "Docker",
            params(
                "pull",
                ProgressState::Begin {
                    title: "Pulling".to_owned(),
                },
            )?,
        );
        board.apply(
            &key,
            "Docker",
            params(
                "pull",
                ProgressState::Report {
                    message: Some("web".to_owned()),
                    percent: Some(250),
                },
            )?,
        );
        let item = board.items.first().ok_or("the pull is shown")?;
        assert_eq!(item.title, "Docker: Pulling");
        assert_eq!(item.message.as_deref(), Some("web"));
        assert_eq!(item.percent, Some(MAX_PERCENT));
        board.apply(&key, "Docker", params("pull", ProgressState::End)?);
        assert!(board.is_empty());
        Ok(())
    }

    #[test]
    fn reports_without_a_beginning_are_ignored_and_the_board_stays_bounded() -> TestResult {
        let mut board = ProgressBoard::default();
        let key = key()?;
        board.apply(
            &key,
            "Docker",
            params(
                "stray",
                ProgressState::Report {
                    message: None,
                    percent: Some(5),
                },
            )?,
        );
        assert!(board.is_empty());
        for index in 0..=MAX_PROGRESS {
            let token = format!("job-{index}");
            board.apply(
                &key,
                "Docker",
                params(
                    &token,
                    ProgressState::Begin {
                        title: token.clone(),
                    },
                )?,
            );
        }
        assert_eq!(board.items.len(), MAX_PROGRESS);
        assert_eq!(
            board
                .items
                .first()
                .map(|item| item.token.to_string())
                .as_deref(),
            Some("job-1")
        );
        board.forget(&key);
        assert!(board.is_empty());
        Ok(())
    }
}
