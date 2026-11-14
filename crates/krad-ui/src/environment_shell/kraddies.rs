mod bus;
mod clock;
mod den_card;
mod editor;
mod farewell;
mod layout;
mod mirror;
mod motion;
mod panel;
mod profile;
mod rows;
mod segmented;
mod shelf;
mod studio;
mod tile;
mod transfer;

use gpui_kit::{Context, Task};
use krad_domain::{AgentState, EnvironmentName, KraddieDesign, KraddieId, KraddieMood, SessionId};
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use super::EnvironmentShell;
use crate::kraddie_avatar::KraddieSprite;

pub(super) use clock::next_run;
pub(super) use editor::KraddieEditor;
pub(super) use studio::KraddieStudio;

pub(super) use super::runtime_view::{AutomationEntry, KraddieEntry};

type MoodSprites = [Rc<KraddieSprite>; KraddieMood::ALL.len()];

#[derive(Default)]
pub(super) struct KraddieSprites(HashMap<KraddieId, (KraddieDesign, MoodSprites)>);

impl KraddieSprites {
    fn refresh(&mut self, entries: &[KraddieEntry]) -> Vec<KraddieId> {
        let gone: Vec<KraddieId> = self
            .0
            .keys()
            .filter(|id| !entries.iter().any(|entry| entry.kraddie.id() == **id))
            .cloned()
            .collect();
        for id in &gone {
            self.0.remove(id);
        }
        for entry in entries {
            let design = &entry.kraddie.design;
            let drawn = self
                .0
                .get(&entry.kraddie.id())
                .is_some_and(|(known, _)| known == design);
            if drawn {
                continue;
            }
            let sprites = KraddieMood::ALL.map(|mood| Rc::new(KraddieSprite::new(design, mood)));
            self.0.insert(entry.kraddie.id(), (design.clone(), sprites));
        }
        gone
    }

    fn of(&self, entry: &KraddieEntry, mood: KraddieMood) -> Option<&Rc<KraddieSprite>> {
        let (_, sprites) = self.0.get(&entry.kraddie.id())?;
        sprites.get(mood.index())
    }
}

pub(super) struct KraddiesUi {
    sprites: KraddieSprites,
    started: Instant,
    pub(super) editor: Option<KraddieEditor>,
    pub(super) studio: Option<KraddieStudio>,
    pub(super) profile: profile::ProfileUi,
    pub(super) memory: super::kraddie_memory::KraddieMemoryUi,
    deleting: Option<KraddieId>,
    animated: Cell<bool>,
    ticker: Option<Task<()>>,
    placeholder: std::cell::OnceCell<Rc<KraddieSprite>>,
}

impl Default for KraddiesUi {
    fn default() -> Self {
        Self {
            sprites: KraddieSprites::default(),
            started: Instant::now(),
            editor: None,
            studio: None,
            profile: profile::ProfileUi::default(),
            memory: super::kraddie_memory::KraddieMemoryUi::default(),
            deleting: None,
            animated: Cell::new(false),
            ticker: None,
            placeholder: std::cell::OnceCell::new(),
        }
    }
}

impl KraddiesUi {
    pub(super) fn sprite(&self, entry: &KraddieEntry, mood: KraddieMood) -> Rc<KraddieSprite> {
        self.sprites
            .of(entry, mood)
            .map_or_else(|| self.placeholder(), Rc::clone)
    }

    pub(super) fn redraw(&mut self, entries: &[KraddieEntry]) -> Vec<KraddieId> {
        self.sprites.refresh(entries)
    }

    pub(super) fn frame(&self) -> u64 {
        krad_domain::animation_frame(self.started.elapsed())
    }

    pub(super) fn placeholder(&self) -> Rc<KraddieSprite> {
        Rc::clone(self.placeholder.get_or_init(panel::placeholder_sprite))
    }

    pub(super) fn note_drawn(&self, sprite: &KraddieSprite) {
        if sprite.moves() {
            self.animated.set(true);
        }
    }
}

impl EnvironmentShell {
    pub(super) fn start_kraddie_clocks(&mut self, cx: &mut Context<Self>) {
        self.start_kraddie_clock(cx);
    }

    pub(super) fn session_kraddie_look(
        &self,
        env: &EnvironmentName,
        session: SessionId,
        agent: Option<AgentState>,
    ) -> Option<Rc<KraddieSprite>> {
        let Some((entry, outcome)) = self.runtime.kraddies.run_on(env, session) else {
            return self.kraddie_chat_look(env, session);
        };
        let sprite = self
            .kraddies
            .sprite(entry, KraddieMood::of_run(agent, Some(outcome)));
        self.kraddies.note_drawn(&sprite);
        Some(sprite)
    }

    pub(super) fn kraddies_frame(&self) -> u64 {
        self.kraddies.frame()
    }

    pub(super) fn kraddie_mood(&self, entry: &KraddieEntry) -> KraddieMood {
        let Some(run) = entry.latest_run() else {
            return KraddieMood::Idle;
        };
        if !run.outcome.is_live() {
            return KraddieMood::of_outcome(&run.outcome);
        }
        let agent = run.session.as_ref().and_then(|link| {
            let live = self.envs.get(&link.environment)?;
            live.data
                .nodes
                .iter()
                .find(|node| node.id == link.terminal)
                .map(|node| node.state)
        });
        KraddieMood::of_run(agent, Some(&run.outcome))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{AgentPreset, Kraddie};

    fn entry(name: &str) -> KraddieEntry {
        let name = name.parse().unwrap_or_else(|_| unreachable!());
        KraddieEntry::new(Kraddie::new(name, AgentPreset::ClaudeCode), Vec::new())
    }

    #[test]
    fn sprites_are_drawn_once_per_design_and_dropped_with_their_kraddie() {
        let mut sprites = KraddieSprites::default();
        let (amber, milo) = (entry("amber"), entry("Milo"));
        let both = [amber.clone(), milo.clone()];
        let nobody: [KraddieId; 0] = [];
        assert_eq!(sprites.refresh(&both), nobody);
        let drawn = sprites.of(&amber, KraddieMood::Idle).map(Rc::clone);
        assert_eq!(sprites.refresh(&both), nobody);
        let again = sprites.of(&amber, KraddieMood::Idle).map(Rc::clone);
        assert!(drawn
            .zip(again)
            .is_some_and(|(drawn, again)| Rc::ptr_eq(&drawn, &again)));
        assert_eq!(
            sprites.refresh(std::slice::from_ref(&amber)),
            [milo.kraddie.id()]
        );
        assert!(sprites.of(&milo, KraddieMood::Idle).is_none());
    }
}
