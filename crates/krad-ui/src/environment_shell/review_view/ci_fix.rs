use gpui_kit::{div, px, rgb, AnyElement, Context, ElementId, IntoElement, ParentElement, Styled};
use krad_domain::review::{CiStatus, Job, JobId};
use krad_domain::ToastKind;
use krad_protocol::{FixOutcome, ReviewAct};

use super::detail::RequestDetail;
use super::{ci_glyph, Remote};
use crate::components::{Button, ButtonVariant};
use crate::environment_shell::EnvironmentShell;
use crate::theme::{palette, ICON_SM, SPACE_2, TEXT_XS};

const BTN_FIX_JOB: &str = "review-fix-job";
const FIX_LABEL: &str = "Fix with agent";
const FIX_TOOLTIP: &str = "Send the failing step's log to the agent working on this branch";
const READING_LOG_LABEL: &str = "Reading the log…";
const FIX_ROW_H: f32 = 28.0;
const SENT_PREFIX: &str = "Sent the failure to the agent on ";
const STARTED_PREFIX: &str = "Started an agent on ";
const NO_REPO_TEXT: &str = "The group is not a git repository.";
const NO_BRANCH_PREFIX: &str = "Fetch the branch first: no local or remote branch is named ";
const NO_CHAT_TEXT: &str = "Could not open a chat for the fix.";
const FORK_PREFIX: &str =
    "The branch lives in a fork, not in this group's repository; check it out from ";
const UNNAMED_FORK: &str = "the fork";

impl EnvironmentShell {
    pub(super) fn failed_job_rows(
        detail: &RequestDetail,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Remote::Ready(jobs) = &detail.jobs else {
            return Vec::new();
        };
        let pal = palette(cx);
        failed_jobs(jobs)
            .map(|job| {
                let action = if detail.fixing.contains(&job.id) {
                    div()
                        .text_color(rgb(pal.text_muted))
                        .child(READING_LOG_LABEL)
                        .into_any_element()
                } else {
                    let id = job.id;
                    Button::new(ElementId::from((BTN_FIX_JOB, id.0)))
                        .label(FIX_LABEL)
                        .tooltip(FIX_TOOLTIP)
                        .small()
                        .variant(ButtonVariant::Secondary)
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            this.fix_job_with_agent(id, cx);
                        }))
                        .into_any_element()
                };
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_2))
                    .h(px(FIX_ROW_H))
                    .text_size(px(TEXT_XS))
                    .text_color(rgb(pal.text_strong))
                    .child(ci_glyph(Some(job.status), ICON_SM, pal))
                    .child(div().flex_1().min_w_0().truncate().child(job.name.clone()))
                    .child(action)
                    .into_any_element()
            })
            .collect()
    }

    fn fix_job_with_agent(&mut self, job: JobId, cx: &mut Context<Self>) {
        let Some((root, request)) = self.open_target() else {
            return;
        };
        let Some(detail) = self.review.detail.as_mut() else {
            return;
        };
        if !detail.fixing.insert(job) {
            return;
        }
        let env = self.active.clone();
        let harness = self.default_chat_harness(&env, cx);
        let act = ReviewAct::FixJob {
            request,
            job,
            harness,
        };
        self.post_review(root, act, cx);
        cx.notify();
    }

    pub(super) fn fix_answered(&mut self, job: JobId, fix: FixOutcome, cx: &mut Context<Self>) {
        if let Some(detail) = self.review.detail.as_mut() {
            detail.fixing.remove(&job);
        }
        let (kind, text) = match fix {
            FixOutcome::Sent { branch } => (ToastKind::Info, format!("{SENT_PREFIX}{branch}")),
            FixOutcome::Started { branch } => {
                (ToastKind::Success, format!("{STARTED_PREFIX}{branch}"))
            }
            FixOutcome::InFork { fork } => {
                let fork = fork.map_or_else(|| UNNAMED_FORK.to_owned(), |fork| fork.to_string());
                (ToastKind::Info, format!("{FORK_PREFIX}{fork}"))
            }
            FixOutcome::NoRepository => (ToastKind::Warning, NO_REPO_TEXT.to_owned()),
            FixOutcome::NoBranch { branch } => {
                (ToastKind::Warning, format!("{NO_BRANCH_PREFIX}{branch}"))
            }
            FixOutcome::NoChat => (ToastKind::Warning, NO_CHAT_TEXT.to_owned()),
            FixOutcome::Failed { message } => (ToastKind::Error, message),
            FixOutcome::Unknown => return cx.notify(),
        };
        self.toast(kind, &text, cx);
    }
}

fn failed_jobs(jobs: &[Job]) -> impl Iterator<Item = &Job> {
    jobs.iter().filter(|job| job.status == CiStatus::Failed)
}
