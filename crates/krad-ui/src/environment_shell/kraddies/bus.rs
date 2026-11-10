use futures::channel::oneshot::Sender as ReplySender;
use gpui_kit::Context;
use krad_agents::{
    AutomationInfo, AutomationRunInfo, ErrorCode, KraddieInfo, KraddieMemoryInfo, KraddieRunInfo,
    Request, Response,
};
use krad_domain::{AutomationKey, AutomationQuery, KraddieId, KraddieWake, Switch};

use super::super::EnvironmentShell;
use super::clock::{next_run, next_run_stamp, trigger_label};
use super::{AutomationEntry, KraddieEntry, KraddieWakeOrder};

const NOT_A_KRADDIE_REQUEST: &str = "Not a Kraddie or automation request";
const NO_AUTOMATIONS: &str = "The Kraddie has no automations; add one in the Automations view";
const AMBIGUOUS: &str = "Several Kraddies have an automation of that name; write <kraddie>/<name>";

fn memory_response(answer: Result<KraddieMemoryInfo, (ErrorCode, String)>) -> Response {
    match answer {
        Ok(memory) => Response::KraddieMemory(memory),
        Err((code, message)) => Response::error(code, message),
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn answer_kraddies(
        &mut self,
        request: Request,
        reply: ReplySender<Response>,
        cx: &mut Context<Self>,
    ) {
        let response = match request {
            Request::Kraddies => Response::Kraddies(self.kraddie_infos()),
            Request::RunKraddie { name, prompt } => self.run_kraddie_from_bus(&name, prompt, cx),
            Request::Automations => Response::Automations(self.automation_infos()),
            Request::RunAutomation { name, prompt } => {
                self.run_automation_from_bus(&name, prompt, cx)
            }
            Request::SwitchAutomation { name, switch } => {
                self.switch_automation_from_bus(&name, switch, cx)
            }
            Request::RememberKraddie { name, text } => {
                memory_response(self.remember_from_bus(&name, &text, cx))
            }
            Request::KraddieMemory { name } => memory_response(self.kraddie_memory_from_bus(&name)),
            _ => Response::error(ErrorCode::BadRequest, NOT_A_KRADDIE_REQUEST),
        };
        reply.send(response).unwrap_or_default();
    }

    fn kraddie_infos(&self) -> Vec<KraddieInfo> {
        let opened_at = self.kraddies.opened_at;
        self.kraddies
            .entries
            .iter()
            .map(|entry| {
                let default = entry.default_automation();
                KraddieInfo {
                    id: entry.kraddie.id().to_string(),
                    name: entry.kraddie.name.to_string(),
                    switch: entry.kraddie.switch,
                    harness: self
                        .settings
                        .acp
                        .harness_name(entry.kraddie.harness)
                        .into_owned(),
                    schedule: default
                        .and_then(|routine| routine.automation.schedules().next().copied())
                        .map(super::clock::schedule_label),
                    next_run: default
                        .and_then(|routine| next_run(entry, routine, opened_at))
                        .as_ref()
                        .map(next_run_stamp),
                    last_outcome: entry.latest_run().map(|run| run.outcome.label().to_owned()),
                }
            })
            .collect()
    }

    pub(in crate::environment_shell) fn automation_info(
        &self,
        kraddie: &KraddieEntry,
        entry: &AutomationEntry,
    ) -> AutomationInfo {
        AutomationInfo {
            kraddie: kraddie.kraddie.id().to_string(),
            id: entry.automation.id.0,
            name: entry.automation.name.to_string(),
            switch: entry.automation.switch,
            triggers: entry
                .automation
                .triggers
                .iter()
                .map(|trigger| trigger_label(*trigger))
                .collect(),
            next_run: next_run(kraddie, entry, self.kraddies.opened_at)
                .as_ref()
                .map(next_run_stamp),
            last_outcome: entry
                .history
                .latest()
                .map(|run| run.outcome.label().to_owned()),
            runs: entry.history.len(),
        }
    }

    fn automation_infos(&self) -> Vec<AutomationInfo> {
        self.kraddies
            .entries
            .iter()
            .flat_map(|kraddie| {
                kraddie
                    .automations
                    .iter()
                    .map(move |entry| self.automation_info(kraddie, entry))
            })
            .collect()
    }

    fn find_automation(&self, asked: &str) -> Result<AutomationKey, Response> {
        let query = AutomationQuery::from(asked);
        let found: Vec<AutomationKey> = self
            .kraddies
            .entries
            .iter()
            .filter(|kraddie| {
                query
                    .kraddie
                    .is_none_or(|name| kraddie.kraddie.name.matches(name))
            })
            .flat_map(|kraddie| {
                kraddie
                    .automations
                    .iter()
                    .filter(|entry| entry.automation.name.matches(query.name))
                    .map(|entry| kraddie.key(entry.automation.id))
            })
            .collect();
        match found.as_slice() {
            [key] => Ok(key.clone()),
            [] => Err(Response::error(
                ErrorCode::NotFound,
                format!("No automation named {asked}"),
            )),
            _ => Err(Response::error(ErrorCode::BadRequest, AMBIGUOUS)),
        }
    }

    fn run_automation_from_bus(
        &mut self,
        name: &str,
        prompt: Option<String>,
        cx: &mut Context<Self>,
    ) -> Response {
        match self.find_automation(name) {
            Ok(key) => self.run_from_bus(&key, prompt, cx),
            Err(response) => response,
        }
    }

    fn switch_automation_from_bus(
        &mut self,
        name: &str,
        switch: Switch,
        cx: &mut Context<Self>,
    ) -> Response {
        let key = match self.find_automation(name) {
            Ok(key) => key,
            Err(response) => return response,
        };
        self.switch_automation(&key, switch, cx);
        match self.kraddies.automation(&key) {
            Some((kraddie, entry)) => {
                Response::AutomationSwitched(Box::new(self.automation_info(kraddie, entry)))
            }
            None => Response::error(ErrorCode::NotFound, format!("No automation named {name}")),
        }
    }

    fn run_kraddie_from_bus(
        &mut self,
        name: &str,
        prompt: Option<String>,
        cx: &mut Context<Self>,
    ) -> Response {
        let Some(kraddie) = self
            .kraddies
            .entries
            .iter()
            .find(|entry| entry.kraddie.name.matches(name))
        else {
            return Response::error(ErrorCode::NotFound, format!("No Kraddie named {name}"));
        };
        let Some(key) = kraddie
            .default_automation()
            .map(|entry| kraddie.key(entry.automation.id))
        else {
            return Response::error(ErrorCode::NotFound, NO_AUTOMATIONS);
        };
        match self.run_from_bus(&key, prompt, cx) {
            Response::AutomationStarted(started) => Response::KraddieStarted(KraddieRunInfo {
                kraddie: started.kraddie,
                run: started.run,
            }),
            other => other,
        }
    }

    fn run_from_bus(
        &mut self,
        key: &AutomationKey,
        prompt: Option<String>,
        cx: &mut Context<Self>,
    ) -> Response {
        let order = KraddieWakeOrder {
            key: key.clone(),
            wake: KraddieWake::Cli,
            prompt,
        };
        let woke = self.wake_kraddie(&order, cx);
        self.save_histories(cx);
        let name = self
            .kraddies
            .automation(key)
            .map_or_default(|(_, entry)| entry.automation.name.to_string());
        match woke {
            Ok(run) => Response::AutomationStarted(AutomationRunInfo {
                kraddie: key.kraddie.to_string(),
                automation: name,
                run: run.0,
            }),
            Err(refusal) => Response::error(refusal.code(), refusal.to_string()),
        }
    }

    pub(in crate::environment_shell) fn run_automation_now(
        &mut self,
        key: &AutomationKey,
        cx: &mut Context<Self>,
    ) {
        let order = KraddieWakeOrder {
            key: key.clone(),
            wake: KraddieWake::Manual,
            prompt: None,
        };
        if let Err(refusal) = self.wake_kraddie(&order, cx) {
            self.toast(&refusal.to_string(), cx);
        }
        self.save_histories(cx);
    }

    pub(in crate::environment_shell) fn run_kraddie_now(
        &mut self,
        id: &KraddieId,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = self.kraddies.entry(id).and_then(|kraddie| {
            kraddie
                .default_automation()
                .map(|entry| kraddie.key(entry.automation.id))
        }) else {
            return self.toast(NO_AUTOMATIONS, cx);
        };
        self.run_automation_now(&key, cx);
    }
}
