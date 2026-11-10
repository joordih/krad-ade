mod bus;
mod draft;
mod draft_sheet;
mod launch;
mod pick;
mod progress;
mod view;
mod view_columns;

use futures::channel::oneshot::Sender as ReplySender;
use gpui_kit::Context;
use krad_agents::Response;
use krad_domain::{EnvironmentName, VariantId, VariantRun, VariantRunId};

use super::EnvironmentShell;

const NO_GROUP_TEXT: &str = "Select a group to compare variants of a task";

type VariantsReply = Option<ReplySender<Response>>;

#[derive(Default)]
pub(super) struct VariantsUi {
    draft: Option<draft::VariantDraft>,
    view: Option<view::VariantsView>,
    progress: Vec<(EnvironmentName, krad_domain::AppEvent)>,
}

fn answer(reply: VariantsReply, response: Response) {
    if let Some(reply) = reply {
        reply.send(response).unwrap_or_default();
    }
}

impl EnvironmentShell {
    fn variant_run(&self, env: &EnvironmentName, run: VariantRunId) -> Option<&VariantRun> {
        self.envs.get(env)?.data.variant_runs.run(run)
    }

    fn variant_run_mut(
        &mut self,
        env: &EnvironmentName,
        run: VariantRunId,
    ) -> Option<&mut VariantRun> {
        self.envs.get_mut(env)?.data.variant_runs.run_mut(run)
    }

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
}
