use gpui_kit::component::input::{InputState, TextareaState};
use gpui_kit::{AppContext as _, Context, Entity, Window};
use krad_domain::{
    launch_budget_warning, AgentPreset, BudgetStatus, GroupId, LaunchBudgetWarning, LaunchEstimate,
    ModelId, ProjectTask, VariantSpec, VariantsOrder, WorkerSurface, MAX_VARIANTS, MIN_VARIANTS,
};
use krad_repo::detect_tasks;

use super::super::review_view::Remote;
use super::super::EnvironmentShell;
use super::NO_GROUP_TEXT;

const PROMPT_ROWS: usize = 5;
const PROMPT_PLACEHOLDER: &str = "The task every variant works on";
const MODEL_PLACEHOLDER: &str = "Model (optional)";
const INSTRUCTION_PLACEHOLDER: &str = "Extra instruction for this variant (optional)";
const NO_AGENTS_TEXT: &str = "Install Claude Code, Codex, opencode or an ACP agent to run variants";
const TEST_TASK_NAME: &str = "test";
const INVALID_MODEL_TEXT: &str = "That model name has characters a model id cannot have";

pub(super) struct DraftRow {
    pub(super) harness: AgentPreset,
    pub(super) model: Entity<InputState>,
    pub(super) instruction: Entity<InputState>,
}

pub(super) struct VariantDraft {
    pub(super) group: GroupId,
    pub(super) prompt: Entity<TextareaState>,
    pub(super) rows: Vec<DraftRow>,
    pub(super) tasks: Remote<Vec<ProjectTask>>,
    pub(super) test: Option<usize>,
    pub(super) warning: Option<String>,
    pub(super) error: Option<String>,
}

pub(super) fn launch_warning_text(group: &str, warning: &LaunchBudgetWarning) -> String {
    let estimate = warning.estimated.map_or_default(|estimated| {
        format!(
            "; {} variants may spend about {estimated} more",
            warning.variants
        )
    });
    match warning.status {
        BudgetStatus::Exceeded => format!(
            "{group} would go over its {} budget ({} spent{estimate})",
            warning.budget, warning.spent
        ),
        BudgetStatus::Approaching | BudgetStatus::Within => format!(
            "{group} is near its {} budget ({} spent{estimate})",
            warning.budget, warning.spent
        ),
    }
}

fn row(harness: AgentPreset, window: &mut Window, cx: &mut Context<EnvironmentShell>) -> DraftRow {
    DraftRow {
        harness,
        model: cx.new(|cx| InputState::new(window, cx).placeholder(MODEL_PLACEHOLDER)),
        instruction: cx.new(|cx| InputState::new(window, cx).placeholder(INSTRUCTION_PLACEHOLDER)),
    }
}

impl EnvironmentShell {
    pub(super) fn variant_harnesses(&self) -> Vec<AgentPreset> {
        self.installed_agents()
            .iter()
            .copied()
            .filter(|preset| preset.is_agent() && preset.worker_surface() == WorkerSurface::Chat)
            .collect()
    }

    pub(in crate::environment_shell) fn open_variants_draft(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(group) = self
            .active_group()
            .map(|group| (group.id, group.root.clone()))
        else {
            return self.toast(NO_GROUP_TEXT, cx);
        };
        let harnesses = self.variant_harnesses();
        let Some(first) = harnesses
            .iter()
            .copied()
            .find(|preset| *preset == self.settings.default_harness)
            .or_else(|| harnesses.first().copied())
        else {
            return self.toast(NO_AGENTS_TEXT, cx);
        };
        let second = harnesses
            .iter()
            .copied()
            .find(|preset| *preset != first)
            .unwrap_or(first);
        let prompt = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(PROMPT_ROWS)
                .placeholder(PROMPT_PLACEHOLDER)
        });
        let focus_target = prompt.clone();
        cx.defer_in(window, move |_, window, cx| {
            focus_target.update(cx, |field, cx| field.focus(window, cx));
        });
        let rows = vec![row(first, window, cx), row(second, window, cx)];
        self.variants.view = None;
        self.variants.draft = Some(VariantDraft {
            group: group.0,
            prompt,
            rows,
            tasks: Remote::Loading,
            test: None,
            warning: None,
            error: None,
        });
        self.refresh_variants_warning();
        self.detect_variant_tasks(group.1, cx);
        cx.notify();
    }

    fn detect_variant_tasks(&self, root: std::path::PathBuf, cx: &mut Context<Self>) {
        let link = self.active_link();
        let detected = cx.background_spawn(async move { detect_tasks(&link, &root) });
        cx.spawn(async move |this, cx| {
            let detection = detected.await;
            this.update(cx, |shell, cx| {
                if let Some(draft) = &mut shell.variants.draft {
                    draft.test = detection
                        .tasks
                        .iter()
                        .position(|task| task.name == TEST_TASK_NAME);
                    draft.tasks = Remote::Ready(detection.tasks);
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn refresh_variants_warning(&mut self) {
        let Some(draft) = &self.variants.draft else {
            return;
        };
        let group = draft.group;
        let variants = draft.rows.len();
        let Some(live) = self.envs.get(&self.active) else {
            return;
        };
        let estimate = LaunchEstimate {
            variants,
            per_variant: live.data.variant_runs.average_variant_cost(group),
        };
        let spent = live.data.group_spend(Some(group)).cost;
        let name = live
            .data
            .group(group)
            .map_or_default(|group| group.name.clone());
        let warning = launch_budget_warning(&self.settings.spending, spent, estimate)
            .map(|warning| launch_warning_text(&name, &warning));
        if let Some(draft) = &mut self.variants.draft {
            draft.warning = warning;
        }
    }

    pub(super) fn add_variant_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let harness = self.variant_harnesses().first().copied();
        let full = self
            .variants
            .draft
            .as_ref()
            .is_none_or(|draft| draft.rows.len() >= MAX_VARIANTS);
        let Some(harness) = harness.filter(|_| !full) else {
            return;
        };
        let added = row(harness, window, cx);
        if let Some(draft) = &mut self.variants.draft {
            draft.rows.push(added);
        }
        self.refresh_variants_warning();
        cx.notify();
    }

    pub(super) fn remove_variant_row(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(draft) = &mut self.variants.draft {
            if draft.rows.len() > MIN_VARIANTS && index < draft.rows.len() {
                draft.rows.remove(index);
            }
        }
        self.refresh_variants_warning();
        cx.notify();
    }

    pub(super) fn launch_drafted_variants(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = &self.variants.draft else {
            return;
        };
        let typed = |field: &Entity<InputState>| field.read(cx).value().trim().to_owned();
        let specs: Result<Vec<VariantSpec>, &str> = draft
            .rows
            .iter()
            .map(|row| {
                let model = typed(&row.model);
                let model = if model.is_empty() {
                    None
                } else {
                    Some(ModelId::parse(&model).ok_or(INVALID_MODEL_TEXT)?)
                };
                Ok(VariantSpec::new(
                    row.harness,
                    model,
                    Some(&typed(&row.instruction)),
                ))
            })
            .collect();
        let test = match (&draft.tasks, draft.test) {
            (Remote::Ready(tasks), Some(index)) => tasks.get(index).cloned(),
            _ => None,
        };
        let prompt = draft.prompt.read(cx).value().to_string();
        let group = draft.group;
        let order = specs.map_err(str::to_owned).and_then(|specs| {
            VariantsOrder::new(&prompt, specs, test).map_err(|error| error.to_string())
        });
        let env = self.active.clone();
        let launched = order.and_then(|order| self.launch_variants(&env, group, order, cx));
        match launched {
            Ok(run) => {
                self.variants.draft = None;
                window.focus(&self.focus, cx);
                self.show_variant_run(group, Some(run), cx);
            }
            Err(error) => {
                if let Some(draft) = &mut self.variants.draft {
                    draft.error = Some(error);
                }
                cx.notify();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::UsdCost;

    fn dollars(amount: f64) -> UsdCost {
        UsdCost::try_from(amount).unwrap_or_default()
    }

    #[test]
    fn the_budget_warning_names_the_group_and_the_estimate_when_there_is_one() {
        let near = LaunchBudgetWarning {
            status: BudgetStatus::Approaching,
            spent: dollars(8.0),
            budget: dollars(10.0),
            variants: 3,
            estimated: Some(dollars(1.5)),
        };
        assert_eq!(
            launch_warning_text("api", &near),
            "api is near its $10.00 budget ($8.00 spent; 3 variants may spend about $1.50 more)"
        );
        let over = LaunchBudgetWarning {
            status: BudgetStatus::Exceeded,
            estimated: None,
            ..near
        };
        assert_eq!(
            launch_warning_text("api", &over),
            "api would go over its $10.00 budget ($8.00 spent)"
        );
    }
}
