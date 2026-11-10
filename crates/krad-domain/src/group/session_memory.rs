use super::new_session::SessionSpec;
use super::Session;
use crate::environment::EnvironmentData;
use crate::ids::{GroupId, SessionId, TerminalId};
use crate::kraddie::KraddieId;
use crate::recall::MemoryOwner;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMentions {
    pub chat: TerminalId,
    pub kraddies: Vec<KraddieId>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionMemory {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kraddie: Option<KraddieId>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mentions: Vec<ChatMentions>,
}

impl SessionMemory {
    #[must_use]
    pub fn is_plain(&self) -> bool {
        *self == Self::default()
    }

    #[must_use]
    pub fn owner(&self) -> MemoryOwner {
        MemoryOwner::from(self.kraddie.clone())
    }

    #[must_use]
    pub fn mentioned_in(&self, chat: TerminalId) -> &[KraddieId] {
        self.mentions
            .iter()
            .find(|mentions| mentions.chat == chat)
            .map_or(&[], |mentions| mentions.kraddies.as_slice())
    }

    pub fn mention(&mut self, chat: TerminalId, kraddies: &[KraddieId]) -> bool {
        if kraddies.is_empty() {
            return false;
        }
        let at = if let Some(at) = self.mentions.iter().position(|found| found.chat == chat) {
            at
        } else {
            self.mentions.push(ChatMentions {
                chat,
                kraddies: Vec::new(),
            });
            self.mentions.len() - 1
        };
        let known = &mut self.mentions[at].kraddies;
        let before = known.len();
        for kraddie in kraddies {
            if !known.contains(kraddie) {
                known.push(kraddie.clone());
            }
        }
        known.len() != before
    }
}

impl EnvironmentData {
    pub fn new_kraddie_session(
        &mut self,
        group: Option<GroupId>,
        kraddie: KraddieId,
        spec: SessionSpec,
    ) -> SessionId {
        let id = self.new_session(group, spec);
        if let Some(session) = self.sessions.iter_mut().find(|session| session.id == id) {
            session.memory.kraddie = Some(kraddie);
        }
        id
    }

    pub fn kraddie_sessions<'a>(
        &'a self,
        kraddie: &'a KraddieId,
    ) -> impl Iterator<Item = &'a Session> {
        self.sessions
            .iter()
            .filter(move |session| session.memory.kraddie.as_ref() == Some(kraddie))
    }

    #[must_use]
    pub fn chat_memory(&self, chat: TerminalId) -> Option<&SessionMemory> {
        let session = self.session_of(chat)?;
        self.session(session).map(|session| &session.memory)
    }

    pub fn rename_kraddie(&mut self, old: &KraddieId, new: &KraddieId) {
        for session in &mut self.sessions {
            let memory = &mut session.memory;
            if memory.kraddie.as_ref() == Some(old) {
                memory.kraddie = Some(new.clone());
            }
            for mentions in &mut memory.mentions {
                for kraddie in &mut mentions.kraddies {
                    if kraddie == old {
                        *kraddie = new.clone();
                    }
                }
            }
        }
    }

    pub fn note_kraddie_mentions(&mut self, chat: TerminalId, kraddies: &[KraddieId]) -> bool {
        let Some(id) = self.session_of(chat) else {
            return false;
        };
        self.sessions
            .iter_mut()
            .find(|session| session.id == id)
            .is_some_and(|session| session.memory.mention(chat, kraddies))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::seed_environment;

    fn id(text: &str) -> KraddieId {
        KraddieId::try_from(text.to_owned()).unwrap_or_else(|_| unreachable!())
    }

    #[test]
    fn a_kraddie_session_names_its_owner_and_keeps_its_chats_mentions() {
        let mut environment = seed_environment();
        let group = environment.add_group(std::path::PathBuf::from("/work/api"), None);
        environment.enter_sessions();
        let session =
            environment.new_kraddie_session(Some(group), id("owl"), SessionSpec::default());
        if let Some(found) = environment
            .sessions
            .iter_mut()
            .find(|found| found.id == session)
        {
            found.tabs = crate::mosaic::MosaicState::with_terminals(&[TerminalId(40)]);
        }
        assert_eq!(environment.session_of(TerminalId(40)), Some(session));
        assert_eq!(
            environment
                .kraddie_sessions(&id("owl"))
                .map(|found| found.id)
                .collect::<Vec<_>>(),
            [session]
        );
        assert!(environment.note_kraddie_mentions(TerminalId(40), &[id("lint"), id("lint")]));
        assert!(!environment.note_kraddie_mentions(TerminalId(40), &[id("lint")]));
        environment.rename_kraddie(&id("lint"), &id("linter"));
        let memory = environment
            .chat_memory(TerminalId(40))
            .cloned()
            .unwrap_or_default();
        assert_eq!(memory.owner(), MemoryOwner::Kraddie(id("owl")));
        assert_eq!(memory.mentioned_in(TerminalId(40)), [id("linter")]);
        assert_eq!(memory.mentioned_in(TerminalId(41)).len(), 0);
    }

    #[test]
    fn a_session_saved_before_kraddie_chats_reads_as_ordinary() -> serde_json::Result<()> {
        let memory: SessionMemory = serde_json::from_str("{}")?;
        assert!(memory.is_plain());
        assert_eq!(memory.owner(), MemoryOwner::Ordinary);
        let mut owned = SessionMemory {
            kraddie: Some(id("owl")),
            ..SessionMemory::default()
        };
        owned.mention(TerminalId(3), &[id("lint")]);
        let text = serde_json::to_string(&owned)?;
        assert_eq!(serde_json::from_str::<SessionMemory>(&text)?, owned);
        Ok(())
    }
}
