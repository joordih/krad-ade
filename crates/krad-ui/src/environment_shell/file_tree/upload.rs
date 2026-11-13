use gpui_kit::{
    div, px, rgb, AnyElement, Context, IntoElement, ParentElement, PromptButton, PromptLevel,
    Styled, Window,
};
use krad_domain::{ToastKind, UploadExisting};
use krad_protocol::{Answer, Command, FileAsk, FileCommand, RequestId, UploadShown};
use std::path::PathBuf;

use super::super::repo_asks::done;

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
const PERCENT: u64 = 100;

pub(in crate::environment_shell) struct UploadJob {
    upload: RequestId,
    items: usize,
    sent: u64,
    total: u64,
    cancelled: bool,
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
        if self.env_is_local(&self.active) || paths.is_empty() || self.file_tree.upload.is_some() {
            return;
        }
        let paths = paths.to_vec();
        let names = paths
            .iter()
            .filter_map(|path| Some(path.file_name()?.to_string_lossy().into_owned()))
            .collect();
        let taken = self.ask_active_file(
            FileAsk::Taken {
                dir: dir.clone(),
                names,
            },
            taken_of,
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let taken = taken.await.unwrap_or_default();
            this.update_in(cx, |shell, window, cx| {
                if taken.is_empty() {
                    shell.start_upload(dir, paths, UploadExisting::Replace, cx);
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
                Ok(REPLACE_ANSWER) => UploadExisting::Replace,
                Ok(SKIP_ANSWER) => UploadExisting::Skip,
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
        sources: Vec<PathBuf>,
        existing: UploadExisting,
        cx: &mut Context<Self>,
    ) {
        let upload = self.runtime.client.next_request();
        self.file_tree.upload = Some(UploadJob {
            upload,
            items: sources.len(),
            sent: 0,
            total: 0,
            cancelled: false,
        });
        let ask = FileAsk::Upload {
            dir,
            sources,
            existing,
        };
        let uploaded = self.ask_file_as(upload, &self.active.clone(), ask, done, cx);
        cx.spawn(async move |this, cx| {
            let uploaded = uploaded.await;
            this.update(cx, |shell, cx| {
                let cancelled = shell
                    .file_tree
                    .upload
                    .take()
                    .is_some_and(|job| job.cancelled);
                match uploaded {
                    Ok(()) => shell.toast(ToastKind::Success, UPLOADED_TEXT, cx),
                    Err(_) if cancelled => shell.toast(ToastKind::Info, CANCELLED_TEXT, cx),
                    Err(error) => shell.toast(ToastKind::Error, &error.to_string(), cx),
                }
                shell.reload_file_tree(cx);
            })
            .unwrap_or_default();
        })
        .detach();
        cx.notify();
    }

    pub(in crate::environment_shell) fn upload_progressed(
        &mut self,
        shown: UploadShown,
        cx: &mut Context<Self>,
    ) {
        let Some(job) = self
            .file_tree
            .upload
            .as_mut()
            .filter(|job| job.upload == shown.upload)
        else {
            return;
        };
        job.sent = shown.sent;
        job.total = shown.total;
        cx.notify();
    }

    fn cancel_upload(&mut self, cx: &mut Context<Self>) {
        let Some(job) = self.file_tree.upload.as_mut() else {
            return;
        };
        job.cancelled = true;
        self.runtime.client.post(Command::File(FileCommand {
            environment: self.active.clone(),
            ask: FileAsk::CancelUpload { upload: job.upload },
        }));
        cx.notify();
    }

    pub(in crate::environment_shell) fn upload_bar(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let job = self.file_tree.upload.as_ref()?;
        let pal = palette(cx);
        let done = percent(job.sent, job.total);
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
                        .on_click(cx.listener(|shell, _, _, cx| shell.cancel_upload(cx))),
                )
                .into_any_element(),
        )
    }
}

fn taken_of(answer: Answer) -> Option<Vec<String>> {
    match answer {
        Answer::Taken { names } => Some(names),
        _ => None,
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
