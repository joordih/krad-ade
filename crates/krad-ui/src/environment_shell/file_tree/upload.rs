use gpui_kit::{
    div, px, rgb, AnyElement, AppContext as _, Context, IntoElement, ParentElement, PromptButton,
    PromptLevel, Styled, Window,
};
use krad_host::{Existing, UploadProgress};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::components::{Button, ButtonVariant};
use crate::environment_shell::EnvironmentShell;
use crate::theme::{palette, LINE_XS, SPACE_1, SPACE_2, TEXT_XS};

const UPLOADED_TEXT: &str = "Uploaded to the host";
const CANCELLED_TEXT: &str = "Upload cancelled";
const REPLACE_TITLE: &str = "Replace files on the host?";
const REPLACE_LABEL: &str = "Replace";
const SKIP_LABEL: &str = "Keep existing";
const CANCEL_LABEL: &str = "Cancel";
const UPLOADING_TEXT: &str = "Uploading";
const BTN_CANCEL_UPLOAD: &str = "file-tree-cancel-upload";
const REPLACE_ANSWER: usize = 0;
const SKIP_ANSWER: usize = 1;
const PROGRESS_TICK: Duration = Duration::from_millis(250);
const PERCENT: u64 = 100;

pub(in crate::environment_shell) struct UploadJob {
    progress: Arc<UploadProgress>,
    items: usize,
}

fn percent(sent: u64, total: u64) -> u64 {
    if total == 0 {
        return 0;
    }
    sent.saturating_mul(PERCENT) / total
}

fn replace_detail(names: &[String]) -> String {
    format!(
        "{} already exist{} in this folder: {}",
        names.len(),
        if names.len() == 1 { "s" } else { "" },
        names.join(", ")
    )
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn drop_on_tree(
        &mut self,
        dir: PathBuf,
        paths: &[PathBuf],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let link = self.active_link();
        if link.is_local() || paths.is_empty() || self.file_tree.upload.is_some() {
            return;
        }
        let paths = paths.to_vec();
        let checked = paths.clone();
        let target = dir.clone();
        let taken = cx.background_spawn(async move {
            checked
                .iter()
                .filter_map(|path| {
                    let name = path.file_name()?;
                    link.symlink_metadata(&target.join(name))
                        .is_ok()
                        .then(|| name.to_string_lossy().into_owned())
                })
                .collect::<Vec<String>>()
        });
        cx.spawn_in(window, async move |this, cx| {
            let taken = taken.await;
            this.update_in(cx, |shell, window, cx| {
                if taken.is_empty() {
                    shell.start_upload(dir, paths, Existing::Replace, cx);
                } else {
                    Self::ask_to_replace(dir, paths, &taken, window, cx);
                }
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn ask_to_replace(
        dir: PathBuf,
        paths: Vec<PathBuf>,
        taken: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let detail = replace_detail(taken);
        let answer = window.prompt(
            PromptLevel::Warning,
            REPLACE_TITLE,
            Some(&detail),
            &[
                PromptButton::ok(REPLACE_LABEL),
                PromptButton::new(SKIP_LABEL),
                PromptButton::cancel(CANCEL_LABEL),
            ],
            cx,
        );
        cx.spawn(async move |this, cx| {
            let existing = match answer.await {
                Ok(REPLACE_ANSWER) => Existing::Replace,
                Ok(SKIP_ANSWER) => Existing::Skip,
                _ => return,
            };
            this.update(cx, |shell, cx| shell.start_upload(dir, paths, existing, cx))
                .unwrap_or_default();
        })
        .detach();
    }

    fn start_upload(
        &mut self,
        dir: PathBuf,
        paths: Vec<PathBuf>,
        existing: Existing,
        cx: &mut Context<Self>,
    ) {
        let link = self.active_link();
        let progress = UploadProgress::new();
        self.file_tree.upload = Some(UploadJob {
            progress: progress.clone(),
            items: paths.len(),
        });
        let sending = progress.clone();
        let uploaded = cx.background_spawn(async move {
            paths
                .iter()
                .try_for_each(|path| link.upload_with(path, &dir, &sending, existing).map(drop))
        });
        let ticker = cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(PROGRESS_TICK).await;
            if this.update(cx, |_, cx| cx.notify()).is_err() {
                break;
            }
        });
        cx.spawn(async move |this, cx| {
            let uploaded = uploaded.await;
            drop(ticker);
            this.update(cx, |shell, cx| {
                shell.file_tree.upload = None;
                match uploaded {
                    Ok(()) => shell.toast(UPLOADED_TEXT, cx),
                    Err(_) if progress.is_cancelled() => shell.toast(CANCELLED_TEXT, cx),
                    Err(error) => shell.toast(&error.to_string(), cx),
                }
                shell.reload_file_tree(cx);
            })
            .unwrap_or_default();
        })
        .detach();
        cx.notify();
    }

    pub(in crate::environment_shell) fn upload_bar(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let job = self.file_tree.upload.as_ref()?;
        let pal = palette(cx);
        let done = percent(job.progress.sent(), job.progress.total());
        let progress = job.progress.clone();
        Some(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(SPACE_2))
                .px(px(SPACE_2))
                .py(px(SPACE_1))
                .bg(pal.bg_hover)
                .text_size(px(TEXT_XS))
                .line_height(px(LINE_XS))
                .text_color(rgb(pal.text_default))
                .child(
                    div()
                        .flex_1()
                        .truncate()
                        .child(format!("{UPLOADING_TEXT} {} · {done}%", job.items)),
                )
                .child(
                    Button::new(BTN_CANCEL_UPLOAD)
                        .label(CANCEL_LABEL)
                        .variant(ButtonVariant::Secondary)
                        .small()
                        .on_click(move |_, _, _| progress.cancel()),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_is_a_whole_percentage_of_the_bytes() {
        assert_eq!(percent(0, 0), 0);
        assert_eq!(percent(512, 1024), 50);
        assert_eq!(percent(1024, 1024), 100);
    }

    #[test]
    fn the_question_names_what_would_be_replaced() {
        assert_eq!(
            replace_detail(&["a.txt".to_owned()]),
            "1 already exists in this folder: a.txt"
        );
        assert_eq!(
            replace_detail(&["a".to_owned(), "b".to_owned()]),
            "2 already exist in this folder: a, b"
        );
    }
}
