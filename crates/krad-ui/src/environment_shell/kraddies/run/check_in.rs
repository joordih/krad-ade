use gpui_kit::Context;
use krad_domain::{KraddieId, KraddieOutcome, KraddieWake, PermissionMode, WorkerSurface};
use std::time::SystemTime;

use super::super::super::EnvironmentShell;
use super::super::RunOwner;
use super::{RunPlace, RunPlan};

impl EnvironmentShell {
    pub(in crate::environment_shell) fn check_in_kraddie(
        &mut self,
        id: &KraddieId,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.kraddies.entry(id) else {
            return;
        };
        let Some(workspace) = entry.kraddie.workspace.clone() else {
            return;
        };
        let harness = entry.kraddie.harness;
        if !self.envs.contains_key(&workspace.environment)
            || harness.worker_surface() != WorkerSurface::Chat
        {
            return;
        }
        let settings = &entry.kraddie.settings;
        let model = settings
            .session_model
            .clone()
            .or_else(|| entry.kraddie.model.clone());
        let (effort, task) = (settings.effort, entry.kraddie.name.to_string());
        let brief = self
            .kraddie_mind(id)
            .check_in_brief(&entry.kraddie, &self.check_in_context(id));
        let env = workspace.environment.clone();
        let group = self
            .repo_group(&env, &workspace.repo)
            .or_else(|| self.add_group_to(&env, workspace.repo.clone(), None, cx));
        let Some(group) = group else {
            return;
        };
        let Some(entry) = self.kraddies.entry_mut(id) else {
            return;
        };
        let run = entry.check_ins.start(
            SystemTime::now(),
            KraddieWake::Rounds,
            KraddieOutcome::Starting,
        );
        let plan = RunPlan {
            branch: String::new(),
            owner: RunOwner::CheckIn(id.clone()),
            run,
            workspace,
            task,
            harness,
            model,
            effort,
            mode: Some(PermissionMode::Plan),
            resume: None,
            place: RunPlace::Group(group),
        };
        tracing::info!(kraddie = %id, run = run.0, "kraddie checks in");
        self.open_run_place(plan, brief, cx);
        cx.notify();
    }
}
