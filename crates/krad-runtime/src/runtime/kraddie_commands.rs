use krad_domain::{AutomationKey, KraddieId, KraddieWake, SessionEdit, Switch};
use krad_protocol::{ErrorCode, KraddieCommand, Reply};

use super::kraddie_bus::OWN_MEMORY;
use super::{refused, Runtime, UNSUPPORTED_COMMAND};
use crate::kraddies::{KraddieWakeError, WakeOrder};

const NO_KRADDIE: &str = "no Kraddie with that id";

impl Runtime {
    pub(super) fn execute_kraddie(&mut self, command: KraddieCommand) -> Reply {
        match command {
            KraddieCommand::Save { kraddie, replaced } => {
                let new = kraddie.id();
                match self.kraddies.save(*kraddie, replaced.as_ref()) {
                    Ok(saved) => {
                        if let Some(old) = saved.renamed {
                            self.rename_kraddie_sessions(&old, &new);
                        }
                        Reply::Accepted
                    }
                    Err(reply) => reply,
                }
            }
            KraddieCommand::Import {
                kraddie,
                automations,
            } => self.kraddies.import(*kraddie, automations),
            KraddieCommand::Delete { kraddie } => self.kraddies.delete(&kraddie),
            KraddieCommand::SaveAutomation {
                kraddie,
                automation,
                previous,
            } => self
                .kraddies
                .save_automation(&kraddie, *automation, previous.as_ref()),
            KraddieCommand::DeleteAutomation { automation } => {
                self.kraddies.delete_automation(&automation)
            }
            KraddieCommand::SwitchAutomation { automation, switch } => {
                self.kraddies.switch_automation(&automation, switch)
            }
            KraddieCommand::Run { automation, prompt } => {
                self.run_now(automation, KraddieWake::Manual, prompt)
            }
            KraddieCommand::RunKraddie { kraddie, prompt } => {
                self.run_kraddie_now(&kraddie, prompt)
            }
            KraddieCommand::OpenChat {
                kraddie,
                environment,
            } => self.open_kraddie_chat_for(&kraddie, &environment),
            KraddieCommand::SetCompass { kraddie, compass } => {
                self.kraddies.set_compass(&kraddie, compass)
            }
            KraddieCommand::Remember { kraddie, text } => self
                .kraddies
                .edit_memory(&kraddie, |memory| memory.add(&text).map(drop)),
            KraddieCommand::ReviseMemory {
                kraddie,
                line,
                text,
            } => self
                .kraddies
                .edit_memory(&kraddie, |memory| memory.revise(&line, &text)),
            KraddieCommand::ForgetMemory { kraddie, line } => self
                .kraddies
                .edit_memory(&kraddie, |memory| memory.forget(&line).map(drop)),
            KraddieCommand::ResetConversation { kraddie } => {
                self.kraddies.reset_conversation(&kraddie)
            }
            KraddieCommand::ShareMemory {
                reader,
                from,
                switch,
            } => self.share_memory(&reader, &from, switch),
            KraddieCommand::ReviseDigest {
                owner,
                digest,
                text,
            } => self.kraddies.revise_digest(&owner, digest, &text),
            KraddieCommand::ForgetDigest { owner, digest } => {
                self.kraddies.forget_digest(&owner, digest)
            }
            KraddieCommand::RecordDigest { owner, draft } => {
                self.kraddies.record_digest(&owner, *draft);
                Reply::Accepted
            }
            KraddieCommand::UseNotes { titles } => {
                self.kraddies.notes = titles;
                Reply::Accepted
            }
            KraddieCommand::Unknown => refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND),
        }
    }

    fn run_now(&mut self, key: AutomationKey, wake: KraddieWake, prompt: Option<String>) -> Reply {
        let order = WakeOrder { key, wake, prompt };
        match self.wake_kraddie(&order) {
            Ok(_) => Reply::Accepted,
            Err(refusal) => refused(refusal.reply_code(), &refusal.to_string()),
        }
    }

    fn share_memory(&mut self, reader: &KraddieId, from: &KraddieId, switch: Switch) -> Reply {
        if reader == from {
            return refused(ErrorCode::Invalid, OWN_MEMORY);
        }
        if self.kraddies.state(reader).is_none() || self.kraddies.state(from).is_none() {
            return refused(ErrorCode::NotFound, NO_KRADDIE);
        }
        self.kraddies.share_memory(reader, from, switch);
        Reply::Accepted
    }

    fn run_kraddie_now(&mut self, kraddie: &KraddieId, prompt: Option<String>) -> Reply {
        let Some(key) = self.kraddies.default_key(kraddie) else {
            let refusal = KraddieWakeError::NoAutomations;
            return refused(refusal.reply_code(), &refusal.to_string());
        };
        self.run_now(key, KraddieWake::Manual, prompt)
    }

    fn rename_kraddie_sessions(&mut self, old: &KraddieId, new: &KraddieId) {
        let names: Vec<_> = self
            .environments
            .iter()
            .map(|data| data.name.clone())
            .collect();
        for environment in names {
            let renamed = SessionEdit::RenameKraddie {
                old: old.clone(),
                new: new.clone(),
            };
            self.environments
                .edit(environment, &renamed.into(), None, &mut self.outbox);
        }
    }
}
