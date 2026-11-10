use super::automation::{
    AppTrigger, Automation, AutomationId, AutomationName, CodeHostTrigger, Trigger, UNTITLED,
};
use super::name::KraddieName;
use super::schedule::KraddieSchedule;
use super::trigger::KraddieEvent;
use crate::code_host::CodeHost;
use serde::Deserialize;

const CODE_HOSTS: [CodeHost; 2] = [CodeHost::Github, CodeHost::Gitlab];

#[derive(Debug, Default, Deserialize)]
pub(super) struct LegacyRoutine {
    #[serde(default)]
    instructions: String,
    #[serde(default)]
    schedule: Option<KraddieSchedule>,
    #[serde(default)]
    events: Vec<KraddieEvent>,
}

#[derive(Deserialize)]
pub(super) struct LegacyFile {
    #[serde(alias = "pet")]
    pub(super) kraddie: LegacyRoutine,
}

fn triggers_of(event: KraddieEvent) -> Vec<Trigger> {
    let on_hosts = |event: CodeHostTrigger| {
        CODE_HOSTS
            .into_iter()
            .map(|host| Trigger::CodeHost { host, event })
            .collect()
    };
    let app = |event: AppTrigger| vec![Trigger::App { event }];
    match event {
        KraddieEvent::AgentFinished => app(AppTrigger::AgentFinished),
        KraddieEvent::GitChanged => app(AppTrigger::GitChanged),
        KraddieEvent::WorktreeCreated => app(AppTrigger::WorktreeCreated),
        KraddieEvent::CiFailed => on_hosts(CodeHostTrigger::CiFailed),
        KraddieEvent::MergeRequestOpened => on_hosts(CodeHostTrigger::RequestOpened),
    }
}

impl LegacyRoutine {
    pub(super) fn into_automation(self, kraddie: &KraddieName) -> Option<Automation> {
        let name = AutomationName::try_from(kraddie.to_string())
            .or_else(|_| AutomationName::try_from(UNTITLED.to_owned()))
            .ok()?;
        let mut automation = Automation::new(AutomationId::FIRST, name);
        automation.instructions = self.instructions;
        automation.triggers.push(Trigger::Manual);
        automation
            .triggers
            .extend(self.schedule.map(|schedule| Trigger::Schedule { schedule }));
        for event in self.events {
            for trigger in triggers_of(event) {
                if !automation.triggers.contains(&trigger) {
                    automation.triggers.push(trigger);
                }
            }
        }
        Some(automation)
    }
}
