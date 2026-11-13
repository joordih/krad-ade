mod draft;
mod draft_sheet;
mod view;
mod view_columns;

use gpui_kit::Context;
use krad_domain::{
    AppEvent, EnvironmentName, EventRecord, ToastKind, VariantId, VariantRun, WinnerOutcome,
};
use krad_protocol::{Command, Reply, VariantAct, VariantCommand};

use super::EnvironmentShell;

const NO_GROUP_TEXT: &str = "Select a group to compare variants of a task";
const READY_PREFIX: &str = "Variants of run ";
const READY_SUFFIX: &str = " are ready to compare";
const APPLIED_TEXT: &str = "applied onto the base worktree; review and commit it there";
const KEPT_TEXT: &str = "kept as its session";

#[derive(Default)]
pub(super) struct VariantsUi {
    draft: Option<draft::VariantDraft>,
    view: Option<view::VariantsView>,
}

impl VariantsUi {
    pub(super) const fn sheet_shown(&self) -> bool {
        self.draft.is_some() || self.view.is_some()
    }
}

impl EnvironmentShell {
    fn variant_label(&self, run: &VariantRun, variant: VariantId) -> String {
        run.variant(variant).map_or_else(
            || variant.to_string(),
            |found| {
                let harness = self.settings.acp.harness_name(found.spec.harness);
                format!("{variant} {}", found.spec.label(&harness))
            },
        )
    }

    fn close_variants_ui(&mut self, cx: &mut Context<Self>) {
        self.variants.draft = None;
        self.variants.view = None;
        cx.notify();
    }

    fn tell_variants(
        &self,
        env: &EnvironmentName,
        act: VariantAct,
        cx: &mut Context<Self>,
        refused: impl FnOnce(&mut Self, String, &mut Context<Self>) + 'static,
    ) {
        let command = Command::Variant(VariantCommand {
            environment: env.clone(),
            act,
        });
        let pending = self
            .runtime
            .client
            .send_as(self.runtime.client.next_request(), command);
        cx.spawn(async move |this, cx| {
            let Ok(Reply::Refused { message, .. }) = pending.await else {
                return;
            };
            this.update(cx, |shell, cx| refused(shell, message, cx))
                .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn variant_event_heard(&mut self, record: &EventRecord) {
        let told = match &record.event {
            AppEvent::VariantsFinished { run } => format!("{READY_PREFIX}{run}{READY_SUFFIX}"),
            AppEvent::VariantWinnerChosen {
                run,
                variant,
                outcome,
            } => {
                let winner = self
                    .envs
                    .get(&record.environment)
                    .and_then(|live| live.data.variant_runs.run(*run))
                    .map_or_else(
                        || variant.to_string(),
                        |found| self.variant_label(found, *variant),
                    );
                let how = match outcome {
                    WinnerOutcome::KeepWorktree => KEPT_TEXT,
                    WinnerOutcome::ApplyOntoBase => APPLIED_TEXT,
                };
                format!("{winner} {how}")
            }
            _ => return,
        };
        self.push_toast(ToastKind::Success, told);
    }
}
