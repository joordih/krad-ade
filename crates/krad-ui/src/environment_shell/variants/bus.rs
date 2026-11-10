use futures::channel::oneshot::Sender as ReplySender;
use gpui_kit::{AppContext as _, Context};
use krad_agents::{ErrorCode, Request, Response, VariantsRequest};
use krad_domain::{
    EnvironmentName, GroupId, ProjectTask, VariantId, VariantRunId, VariantsOrder,
    VariantsOrderError,
};
use krad_repo::{detect_tasks, VariantRemoval};

use super::super::agent_bus::target_group;
use super::super::EnvironmentShell;
use super::answer;
use super::pick::VariantPick;

const NO_ENVIRONMENT: &str = "The caller's environment is not open";
const NO_GROUP: &str = "The group is not in the caller's environment";
const NOT_CONFIRMED: &str =
    "Picking a winner discards the other variants and removes their worktrees; confirm it";
const UNKNOWN_TASK: &str = "The group has no project task named";
const TASK_LIST_SEPARATOR: &str = ", ";
const KNOWN_TASKS: &str = "; known tasks: ";

fn chosen_task(tasks: Vec<ProjectTask>, named: &str) -> Result<ProjectTask, String> {
    let wanted = named.trim();
    let known: Vec<String> = tasks.iter().map(ToString::to_string).collect();
    tasks
        .into_iter()
        .find(|task| task.to_string() == wanted || task.name == wanted)
        .ok_or_else(|| {
            format!(
                "{UNKNOWN_TASK} `{wanted}`{KNOWN_TASKS}{}",
                known.join(TASK_LIST_SEPARATOR)
            )
        })
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn answer_variants(
        &mut self,
        environment: &str,
        caller: Option<GroupId>,
        request: Request,
        reply: ReplySender<Response>,
        cx: &mut Context<Self>,
    ) {
        let env = EnvironmentName::new(environment.to_owned());
        let Some(live) = self.envs.get(&env) else {
            reply
                .send(Response::error(ErrorCode::NotFound, NO_ENVIRONMENT))
                .unwrap_or_default();
            return;
        };
        let response = match request {
            Request::RunVariants(asked) => {
                match target_group(&live.data, caller, asked.group.as_deref()) {
                    Ok(group) => return self.run_variants_from_bus(&env, group, asked, reply, cx),
                    Err(error) => Response::Error(error),
                }
            }
            Request::Variants { run } => {
                let runs = &live.data.variant_runs;
                let shown = match run {
                    Some(run) => runs.run(VariantRunId(run)).cloned().into_iter().collect(),
                    None => caller.map_or_else(
                        || runs.all().to_vec(),
                        |group| runs.of_group(group).cloned().collect(),
                    ),
                };
                Response::Variants(shown)
            }
            Request::PickVariant {
                run,
                variant,
                outcome,
                confirmed,
            } => {
                if confirmed {
                    let (run, variant) = (VariantRunId(run), VariantId(variant));
                    let pick = VariantPick {
                        run,
                        winner: variant,
                        outcome,
                        removal: VariantRemoval::OnlyIfSettled,
                    };
                    return self.pick_variant(&env, pick, Some(reply), cx);
                }
                Response::error(ErrorCode::BadRequest, NOT_CONFIRMED)
            }
            _ => return,
        };
        reply.send(response).unwrap_or_default();
    }

    fn run_variants_from_bus(
        &mut self,
        env: &EnvironmentName,
        group: GroupId,
        asked: VariantsRequest,
        reply: ReplySender<Response>,
        cx: &mut Context<Self>,
    ) {
        let VariantsRequest {
            prompt,
            variants,
            test,
            ..
        } = asked;
        let Some(test) = test else {
            let order = VariantsOrder::new(&prompt, variants, None);
            return self.start_bus_variants(env, group, order, reply, cx);
        };
        let Some(root) = self
            .envs
            .get(env)
            .and_then(|live| live.data.group(group))
            .map(|group| group.root.clone())
        else {
            return answer(Some(reply), Response::error(ErrorCode::NotFound, NO_GROUP));
        };
        let env = env.clone();
        let link = self.env_link(&env);
        let detected = cx.background_spawn(async move { detect_tasks(&link, &root).tasks });
        cx.spawn(async move |this, cx| {
            let tasks = detected.await;
            this.update(cx, |shell, cx| match chosen_task(tasks, &test) {
                Ok(task) => {
                    let order = VariantsOrder::new(&prompt, variants, Some(task));
                    shell.start_bus_variants(&env, group, order, reply, cx);
                }
                Err(error) => answer(Some(reply), Response::error(ErrorCode::BadRequest, error)),
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn start_bus_variants(
        &mut self,
        env: &EnvironmentName,
        group: GroupId,
        order: Result<VariantsOrder, VariantsOrderError>,
        reply: ReplySender<Response>,
        cx: &mut Context<Self>,
    ) {
        let response = order
            .map_err(|error| Response::error(ErrorCode::BadRequest, error.to_string()))
            .and_then(|order| {
                self.launch_variants(env, group, order, cx)
                    .map_err(|error| Response::error(ErrorCode::Unsupported, error))
            })
            .map_or_else(
                |error| error,
                |run| Response::VariantsStarted { run: run.0 },
            );
        reply.send(response).unwrap_or_default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::TaskSource;

    fn task(name: &str) -> ProjectTask {
        ProjectTask {
            source: TaskSource::Cargo,
            name: name.to_owned(),
            program: "cargo".to_owned(),
            args: vec![name.to_owned()],
        }
    }

    #[test]
    fn the_test_task_is_found_by_its_command_or_its_name() {
        let tasks = || vec![task("build"), task("test")];
        assert_eq!(chosen_task(tasks(), "cargo test"), Ok(task("test")));
        assert_eq!(chosen_task(tasks(), " test "), Ok(task("test")));
        assert_eq!(
            chosen_task(tasks(), "npm test"),
            Err("The group has no project task named `npm test`; known tasks: cargo build, cargo test".to_owned())
        );
    }
}
