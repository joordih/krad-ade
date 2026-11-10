mod activity;
mod compass;
mod day;
mod general;
mod heatmap;
mod hero;
mod mind;
mod page;
mod rounds;
mod sessions;
mod sidebar;

use chrono::{Local, NaiveDate};
use gpui_kit::component::input::{InputState, TextareaState};
use gpui_kit::{AppContext as _, Bounds, Context, Entity, Pixels};
use krad_domain::{
    EnvironmentName, HeatMap, HeatMetric, KraddieActivity, KraddieId, KraddieMind, KraddieRunId,
    TerminalId,
};
use krad_store::KraddieProfileFiles;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::SystemTime;

use super::super::EnvironmentShell;

pub(in crate::environment_shell) use heatmap::HeatHover;
pub(super) use hero::{mood_color, mood_line};
pub(super) use page::width_probe;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::environment_shell) enum ProfileSection {
    #[default]
    Glance,
    Compass,
    Keepsakes,
    Memory,
    Tuning,
}

impl ProfileSection {
    const ALL: [Self; 5] = [
        Self::Glance,
        Self::Compass,
        Self::Keepsakes,
        Self::Memory,
        Self::Tuning,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::Glance => "At a glance",
            Self::Compass => "Compass",
            Self::Keepsakes => "Keepsakes",
            Self::Memory => "Chats and memory",
            Self::Tuning => "Tuning",
        }
    }

    const fn short_label(self) -> &'static str {
        match self {
            Self::Glance => "Glance",
            Self::Compass => "Compass",
            Self::Keepsakes => "Keepsakes",
            Self::Memory => "Chats",
            Self::Tuning => "Tuning",
        }
    }

    const fn hint(self) -> &'static str {
        match self {
            Self::Glance => "Its trail, its day and what comes next",
            Self::Compass => "What it should always keep in mind",
            Self::Keepsakes => "Short notes it carries from run to run",
            Self::Memory => "Its chats, what they left behind and whose memory it reads",
            Self::Tuning => "Agent, reach, sessions and rounds",
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::Glance => 0,
            Self::Compass => 1,
            Self::Keepsakes => 2,
            Self::Memory => 3,
            Self::Tuning => 4,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HeatKey {
    kraddie: KraddieId,
    metric: HeatMetric,
    today: NaiveDate,
    revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EndedRun {
    pub(super) env: EnvironmentName,
    pub(super) terminal: TerminalId,
    pub(super) kraddie: KraddieId,
    pub(super) run: KraddieRunId,
}

struct CompassDraft {
    kraddie: KraddieId,
    text: Entity<TextareaState>,
}

struct LineDraft {
    index: usize,
    text: Entity<InputState>,
}

#[derive(Default)]
pub(in crate::environment_shell) struct ProfileUi {
    pub(super) open: Option<KraddieId>,
    section: ProfileSection,
    metric: HeatMetric,
    minds: HashMap<KraddieId, KraddieMind>,
    activity: HashMap<KraddieId, KraddieActivity>,
    revision: u64,
    heat: RefCell<Option<(HeatKey, Rc<HeatMap>)>>,
    pub(in crate::environment_shell) hover: Option<HeatHover>,
    grid_origin: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub(in crate::environment_shell) width: Rc<Cell<Option<f32>>>,
    pokes: usize,
    open_menu: Option<&'static str>,
    logbook_open: bool,
    compass_saves: usize,
    compass_refused: usize,
    compass: Option<CompassDraft>,
    new_line: Option<Entity<InputState>>,
    editing_line: Option<LineDraft>,
    session_model: Option<(KraddieId, Entity<InputState>)>,
    resetting: Option<KraddieId>,
    check_ins: HashMap<(EnvironmentName, TerminalId), KraddieId>,
    last_check_in: HashMap<KraddieId, SystemTime>,
    pub(super) ended: Vec<EndedRun>,
    dirty_minds: HashSet<KraddieId>,
    dirty_activity: HashSet<KraddieId>,
}

fn today() -> NaiveDate {
    Local::now().date_naive()
}

impl ProfileUi {
    fn mind(&self, id: &KraddieId) -> KraddieMind {
        self.minds.get(id).cloned().unwrap_or_default()
    }

    fn activity_of(&self, id: &KraddieId) -> Option<&KraddieActivity> {
        self.activity.get(id)
    }

    fn heat_map(&self, id: &KraddieId) -> Rc<HeatMap> {
        let key = HeatKey {
            kraddie: id.clone(),
            metric: self.metric,
            today: today(),
            revision: self.revision,
        };
        let mut cache = self.heat.borrow_mut();
        if let Some((cached, map)) = cache.as_ref() {
            if *cached == key {
                return Rc::clone(map);
            }
        }
        let empty = KraddieActivity::default();
        let activity = self.activity.get(id).unwrap_or(&empty);
        let map = Rc::new(HeatMap::build(activity, key.today, key.metric));
        *cache = Some((key, Rc::clone(&map)));
        map
    }

    fn toggle_menu(&mut self, menu: &'static str) {
        self.open_menu = if self.open_menu == Some(menu) {
            None
        } else {
            Some(menu)
        };
    }

    fn touched_activity(&mut self, id: &KraddieId) {
        self.revision = self.revision.wrapping_add(1);
        self.dirty_activity.insert(id.clone());
    }

    pub(super) fn rename(&mut self, old: &KraddieId, new: &KraddieId) {
        if let Some(mind) = self.minds.remove(old) {
            self.minds.insert(new.clone(), mind);
        }
        if let Some(activity) = self.activity.remove(old) {
            self.activity.insert(new.clone(), activity);
        }
        if self.open.as_ref() == Some(old) {
            self.open = Some(new.clone());
        }
        self.revision = self.revision.wrapping_add(1);
    }

    pub(super) fn forget_check_in(&mut self, env: &EnvironmentName, terminal: TerminalId) {
        self.check_ins.remove(&(env.clone(), terminal));
    }

    pub(super) fn forget(&mut self, id: &KraddieId) {
        self.minds.remove(id);
        self.activity.remove(id);
        self.last_check_in.remove(id);
        if self.open.as_ref() == Some(id) {
            self.open = None;
        }
    }
}

impl EnvironmentShell {
    pub(super) fn load_kraddie_profiles(&mut self, cx: &mut Context<Self>) {
        let Some(store) = self.kraddies.shelf.clone() else {
            return;
        };
        let ids: Vec<KraddieId> = self
            .kraddies
            .entries
            .iter()
            .map(|entry| entry.kraddie.id())
            .collect();
        let read = cx.background_spawn(async move {
            ids.into_iter()
                .map(|id| {
                    let files = store.read_profile(&id);
                    (id, files)
                })
                .collect::<Vec<(KraddieId, KraddieProfileFiles)>>()
        });
        cx.spawn(async move |this, cx| {
            let profiles = read.await;
            this.update(cx, |shell, cx| {
                let today = today();
                for (id, mut files) in profiles {
                    files.activity.prune(today);
                    shell.kraddies.profile.minds.insert(id.clone(), files.mind);
                    shell.kraddies.profile.activity.insert(id, files.activity);
                }
                shell.kraddies.profile.revision = shell.kraddies.profile.revision.wrapping_add(1);
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn save_kraddie_profiles(&mut self, cx: &mut Context<Self>) {
        let Some(store) = self.kraddies.shelf.clone() else {
            return;
        };
        let profile = &mut self.kraddies.profile;
        let minds: Vec<(KraddieId, KraddieMind)> = std::mem::take(&mut profile.dirty_minds)
            .into_iter()
            .map(|id| {
                let mind = profile.mind(&id);
                (id, mind)
            })
            .collect();
        let activity: Vec<(KraddieId, KraddieActivity)> =
            std::mem::take(&mut profile.dirty_activity)
                .into_iter()
                .filter_map(|id| {
                    let activity = profile.activity.get(&id)?.clone();
                    Some((id, activity))
                })
                .collect();
        if minds.is_empty() && activity.is_empty() {
            return;
        }
        cx.background_spawn(async move {
            for (id, mind) in minds {
                if let Err(error) = store
                    .write_compass(&id, &mind.compass)
                    .and_then(|()| store.write_memory(&id, &mind.memory))
                {
                    tracing::warn!(kraddie = %id, %error, "kraddie compass or memory not saved");
                }
            }
            for (id, activity) in activity {
                if let Err(error) = store.write_activity(&id, &activity) {
                    tracing::warn!(kraddie = %id, %error, "kraddie activity not saved");
                }
            }
        })
        .detach();
    }

    pub(in crate::environment_shell) fn open_kraddie_page(
        &mut self,
        id: &KraddieId,
        cx: &mut Context<Self>,
    ) {
        self.kraddies.profile.open = Some(id.clone());
        self.kraddies.profile.section = ProfileSection::Glance;
        self.kraddies.profile.logbook_open = false;
        self.kraddies.profile.hover = None;
        self.kraddies.profile.resetting = None;
        self.hide_extension_main_view();
        self.set_view(krad_domain::EnvironmentView::Kraddies, cx);
        cx.notify();
    }

    fn close_kraddie_page(&mut self, cx: &mut Context<Self>) {
        self.kraddies.profile.open = None;
        self.kraddies.profile.compass = None;
        self.kraddies.profile.editing_line = None;
        self.kraddies.profile.new_line = None;
        cx.notify();
    }

    fn edit_mind(
        &mut self,
        id: &KraddieId,
        edit: impl FnOnce(&mut KraddieMind),
        cx: &mut Context<Self>,
    ) {
        let mind = self.kraddies.profile.minds.entry(id.clone()).or_default();
        edit(mind);
        self.kraddies.profile.dirty_minds.insert(id.clone());
        self.save_kraddie_profiles(cx);
        cx.notify();
    }

    pub(in crate::environment_shell) fn kraddie_mind(&self, id: &KraddieId) -> KraddieMind {
        self.kraddies.profile.mind(id)
    }
}
