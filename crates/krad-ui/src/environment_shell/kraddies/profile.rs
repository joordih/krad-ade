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
use gpui_kit::{Bounds, Context, Entity, Pixels};
use krad_domain::{HeatMap, HeatMetric, KraddieActivity, KraddieId, KraddieMind};

use super::super::runtime_view::KraddieMinds;
use krad_protocol::KraddieCommand;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

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
}

fn today() -> NaiveDate {
    Local::now().date_naive()
}

impl ProfileUi {
    fn heat_map(&self, id: &KraddieId, minds: &KraddieMinds) -> Rc<HeatMap> {
        let key = HeatKey {
            kraddie: id.clone(),
            metric: self.metric,
            today: today(),
            revision: minds.revision,
        };
        let mut cache = self.heat.borrow_mut();
        if let Some((cached, map)) = cache.as_ref() {
            if *cached == key {
                return Rc::clone(map);
            }
        }
        let empty = KraddieActivity::default();
        let activity = minds.activity(id).unwrap_or(&empty);
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

    pub(super) fn rename(&mut self, old: &KraddieId, new: &KraddieId) {
        if self.open.as_ref() == Some(old) {
            self.open = Some(new.clone());
        }
    }

    pub(super) fn forget(&mut self, id: &KraddieId) {
        if self.open.as_ref() == Some(id) {
            self.open = None;
        }
    }
}

impl EnvironmentShell {
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
        (edit, command): (impl FnOnce(&mut KraddieMind), KraddieCommand),
        cx: &mut Context<Self>,
    ) {
        self.edit_kraddie_book(cx, |book| edit(book.minds.mind_mut(id)));
        self.tell_kraddies(command, cx);
        cx.notify();
    }

    pub(in crate::environment_shell) fn kraddie_mind(&self, id: &KraddieId) -> KraddieMind {
        self.runtime.kraddies.minds.mind(id)
    }
}
