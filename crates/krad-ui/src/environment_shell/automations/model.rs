use chrono::Weekday;
use krad_domain::{
    code_host_name, AppTrigger, AutomationDraft, AutomationKey, CodeHost, CodeHostTrigger,
    IntervalMinutes, KraddieId, KraddieSchedule, KraddieWorkspace, TimeOfDay, Trigger, Weekdays,
};
use std::time::Duration;

use super::super::kraddies::{AutomationEntry, KraddieEntry};
use super::wording::{
    app_sentence, code_host_sentence, cues_summary, schedule_sentence, tone, Tone,
};
use super::{AutomationMenu, CodeHostLogins};
use crate::components::IconName;

const PRESET_MINUTES: [u32; 4] = [15, 30, 60, 360];
const PRESET_HOUR: u8 = 9;
const CLOCK_LABEL: &str = "On the clock";
const CLOCK_DETAIL: &str = "A rhythm or a fixed hour";
const APP_LABEL: &str = "Inside the app";
const APP_DETAIL: &str = "Agents, files and worktrees";
const HOST_DETAIL: &str = "Pull requests and checks";
const GITLAB_DETAIL: &str = "Merge requests and pipelines";
const SIGNED_OUT_DETAIL: &str = "Sign in under Settings first";
const MANUAL_LABEL: &str = "By hand";
const MANUAL_DETAIL: &str = "The run button or krad automation run";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LastRun {
    pub(super) tone: Tone,
    pub(super) age: Option<Duration>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Duty {
    Working,
    Resting,
    OwnerResting,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AutomationCard {
    pub(super) key: AutomationKey,
    pub(super) name: String,
    pub(super) owner: String,
    pub(super) duty: Duty,
    pub(super) running: bool,
    pub(super) cues: String,
    pub(super) next_in: Option<Duration>,
    pub(super) last: Option<LastRun>,
    pub(super) last_label: Option<String>,
}

impl AutomationCard {
    #[must_use]
    pub(super) fn on_duty(&self) -> bool {
        self.duty == Duty::Working
    }

    #[must_use]
    pub(super) fn switched_on(&self) -> bool {
        self.duty != Duty::Resting
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Tally {
    pub(super) on_duty: usize,
    pub(super) resting: usize,
    pub(super) running: usize,
}

pub(super) struct CardClock<'a> {
    pub(super) next_in: &'a dyn Fn(&KraddieEntry, &AutomationEntry) -> Option<Duration>,
    pub(super) age: &'a dyn Fn(std::time::SystemTime) -> Option<Duration>,
}

fn matches(filter: &str, kraddie: &KraddieEntry, entry: &AutomationEntry) -> bool {
    filter.is_empty()
        || entry
            .automation
            .name
            .as_str()
            .to_lowercase()
            .contains(filter)
        || kraddie
            .kraddie
            .name
            .as_str()
            .to_lowercase()
            .contains(filter)
        || cues_summary(&entry.automation.triggers)
            .to_lowercase()
            .contains(filter)
}

fn card(kraddie: &KraddieEntry, entry: &AutomationEntry, clock: &CardClock<'_>) -> AutomationCard {
    let key = kraddie.key(entry.automation.id);
    let latest = entry.history.latest();
    let running = latest.is_some_and(|run| run.outcome.is_live());
    AutomationCard {
        name: entry.automation.name.to_string(),
        owner: kraddie.kraddie.name.to_string(),
        duty: match (
            entry.automation.switch.is_on(),
            kraddie.kraddie.switch.is_on(),
        ) {
            (false, _) => Duty::Resting,
            (true, false) => Duty::OwnerResting,
            (true, true) => Duty::Working,
        },
        running,
        cues: cues_summary(&entry.automation.triggers),
        next_in: (clock.next_in)(kraddie, entry),
        last: latest.map(|run| LastRun {
            tone: tone(Some(&run.outcome)),
            age: (clock.age)(run.started),
        }),
        last_label: latest.map(|run| run.outcome.label().to_owned()),
        key,
    }
}

fn order(card: &AutomationCard) -> (bool, bool, Duration, String) {
    (
        !card.running,
        !card.on_duty(),
        card.next_in.unwrap_or(Duration::MAX),
        card.name.to_lowercase(),
    )
}

#[must_use]
pub(super) fn automation_cards(
    kraddies: &[KraddieEntry],
    filter: &str,
    clock: &CardClock<'_>,
) -> Vec<AutomationCard> {
    let filter = filter.trim().to_lowercase();
    let mut cards: Vec<AutomationCard> = kraddies
        .iter()
        .flat_map(|kraddie| {
            kraddie
                .automations
                .iter()
                .map(move |entry| (kraddie, entry))
        })
        .filter(|(kraddie, entry)| matches(&filter, kraddie, entry))
        .map(|(kraddie, entry)| card(kraddie, entry, clock))
        .collect();
    cards.sort_by_cached_key(order);
    cards
}

#[must_use]
pub(super) fn filtered_cards(cards: &[AutomationCard], filter: &str) -> Vec<AutomationCard> {
    let filter = filter.trim().to_lowercase();
    cards
        .iter()
        .filter(|card| {
            [&card.name, &card.owner, &card.cues]
                .iter()
                .any(|text| text.to_lowercase().contains(&filter))
        })
        .cloned()
        .collect()
}

#[must_use]
pub(super) fn tally(cards: &[AutomationCard]) -> Tally {
    cards.iter().fold(Tally::default(), |mut tally, card| {
        if card.on_duty() {
            tally.on_duty += 1;
        } else {
            tally.resting += 1;
        }
        if card.running {
            tally.running += 1;
        }
        tally
    })
}

#[must_use]
pub(super) fn owner_for(
    kraddies: &[KraddieEntry],
    here: Option<&KraddieWorkspace>,
) -> Option<KraddieId> {
    kraddies
        .iter()
        .find(|kraddie| here.is_some_and(|here| kraddie.kraddie.workspace.as_ref() == Some(here)))
        .or_else(|| kraddies.first())
        .map(|kraddie| kraddie.kraddie.id())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CueShelf {
    Clock,
    Host(CodeHost),
    App,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum CueEntry {
    Shelf {
        shelf: CueShelf,
        label: &'static str,
        detail: &'static str,
        glyph: IconName,
        locked: bool,
    },
    Direct {
        label: String,
        detail: Option<&'static str>,
        glyph: IconName,
        picked: bool,
        pick: AutomationMenu,
    },
}

impl CueShelf {
    #[must_use]
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Clock => CLOCK_LABEL,
            Self::Host(host) => code_host_name(host),
            Self::App => APP_LABEL,
        }
    }
}

fn schedule_presets() -> Vec<KraddieSchedule> {
    let every = PRESET_MINUTES
        .into_iter()
        .filter_map(|minutes| IntervalMinutes::try_from(minutes).ok())
        .map(|minutes| KraddieSchedule::Every { minutes });
    let at = TimeOfDay::new(PRESET_HOUR, 0)
        .ok()
        .into_iter()
        .flat_map(|time| {
            let monday = Weekdays::new(&[Weekday::Mon]).ok();
            [Some(Weekdays::EVERY_DAY), Some(Weekdays::WORKDAYS), monday]
                .into_iter()
                .flatten()
                .map(move |days| KraddieSchedule::At { time, days })
        });
    every.chain(at).collect()
}

fn toggled(label: String, glyph: IconName, trigger: Trigger, draft: &AutomationDraft) -> CueEntry {
    CueEntry::Direct {
        label,
        detail: None,
        glyph,
        picked: draft.triggers.contains(&trigger),
        pick: AutomationMenu::AddTrigger(trigger),
    }
}

#[must_use]
pub(super) fn cue_root(draft: &AutomationDraft, logins: CodeHostLogins) -> Vec<CueEntry> {
    let host = |host: CodeHost, signed_in: bool, detail: &'static str| CueEntry::Shelf {
        shelf: CueShelf::Host(host),
        label: code_host_name(host),
        detail: if signed_in { detail } else { SIGNED_OUT_DETAIL },
        glyph: IconName::PullRequest,
        locked: !signed_in,
    };
    vec![
        CueEntry::Shelf {
            shelf: CueShelf::Clock,
            label: CLOCK_LABEL,
            detail: CLOCK_DETAIL,
            glyph: IconName::Repeat,
            locked: false,
        },
        host(CodeHost::Github, logins.github, HOST_DETAIL),
        host(CodeHost::Gitlab, logins.gitlab, GITLAB_DETAIL),
        CueEntry::Shelf {
            shelf: CueShelf::App,
            label: APP_LABEL,
            detail: APP_DETAIL,
            glyph: IconName::Flash,
            locked: false,
        },
        CueEntry::Direct {
            label: MANUAL_LABEL.to_owned(),
            detail: Some(MANUAL_DETAIL),
            glyph: IconName::Play,
            picked: draft.triggers.contains(&Trigger::Manual),
            pick: AutomationMenu::AddTrigger(Trigger::Manual),
        },
    ]
}

#[must_use]
pub(super) fn cue_shelf(shelf: CueShelf, draft: &AutomationDraft) -> Vec<CueEntry> {
    match shelf {
        CueShelf::Clock => schedule_presets()
            .into_iter()
            .map(|schedule| CueEntry::Direct {
                label: schedule_sentence(schedule),
                detail: None,
                glyph: IconName::Repeat,
                picked: false,
                pick: AutomationMenu::AddSchedule(schedule),
            })
            .collect(),
        CueShelf::Host(host) => CodeHostTrigger::ALL
            .into_iter()
            .map(|event| {
                toggled(
                    code_host_sentence(host, event).to_owned(),
                    IconName::PullRequest,
                    Trigger::CodeHost { host, event },
                    draft,
                )
            })
            .collect(),
        CueShelf::App => AppTrigger::ALL
            .into_iter()
            .map(|event| {
                toggled(
                    app_sentence(event).to_owned(),
                    IconName::Flash,
                    Trigger::App { event },
                    draft,
                )
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{Automation, AutomationId, KraddieHistory, KraddieOutcome, KraddieWake};
    use std::time::SystemTime;

    fn kraddie(name: &str, automations: &[&str]) -> KraddieEntry {
        let kraddie = krad_domain::Kraddie::new(
            name.parse().unwrap_or_else(|_| unreachable!()),
            krad_domain::AgentPreset::ClaudeCode,
        );
        let automations = automations
            .iter()
            .zip(1..)
            .map(|(name, id)| AutomationEntry {
                automation: Automation::new(
                    AutomationId(id),
                    name.parse().unwrap_or_else(|_| unreachable!()),
                ),
                history: KraddieHistory::default(),
            })
            .collect();
        KraddieEntry::new(kraddie, automations)
    }

    fn cards(kraddies: &[KraddieEntry], filter: &str) -> Vec<AutomationCard> {
        let clock = CardClock {
            next_in: &|_, entry| {
                (entry.automation.name.as_str() == "Recap").then_some(Duration::from_secs(90))
            },
            age: &|_| Some(Duration::from_secs(60)),
        };
        automation_cards(kraddies, filter, &clock)
    }

    #[test]
    fn running_cards_lead_then_the_soonest_then_by_name() {
        let mut kraddies = [
            kraddie("Owl", &["Sweep", "Recap"]),
            kraddie("Fox", &["Audit", "Probe"]),
        ];
        kraddies[1].automations[1].history.start(
            SystemTime::now(),
            KraddieWake::Manual,
            KraddieOutcome::Starting,
        );
        let names: Vec<String> = cards(&kraddies, "")
            .into_iter()
            .map(|card| card.name)
            .collect();
        assert_eq!(names, ["Probe", "Recap", "Audit", "Sweep"]);
        let all = cards(&kraddies, "");
        assert_eq!(
            tally(&all),
            Tally {
                on_duty: 4,
                resting: 0,
                running: 1
            }
        );
        assert_eq!(all[1].next_in, Some(Duration::from_secs(90)));
        assert_eq!(all[1].owner, "Owl");
    }

    #[test]
    fn the_filter_matches_the_name_the_owner_or_the_cue() {
        let kraddies = [kraddie("Owl", &["Sweep"]), kraddie("Fox", &["Audit"])];
        let names = |filter: &str| -> Vec<String> {
            cards(&kraddies, filter)
                .into_iter()
                .map(|card| card.name)
                .collect()
        };
        assert_eq!(names(" fox "), ["Audit"]);
        assert_eq!(names("SWE"), ["Sweep"]);
        assert_eq!(names("only when you ask").len(), 2);
        assert_eq!(names("zzz"), Vec::<String>::new());
    }

    #[test]
    fn filtering_the_drawn_cards_matches_building_them_filtered_and_asks_the_clock_once() {
        let kraddies = [
            kraddie("Owl", &["Sweep", "Recap"]),
            kraddie("Fox", &["Audit"]),
        ];
        let asked = std::cell::Cell::new(0);
        let clock = CardClock {
            next_in: &|_, _| {
                asked.set(asked.get() + 1);
                None
            },
            age: &|_| None,
        };
        let everything = automation_cards(&kraddies, "", &clock);
        assert_eq!(asked.get(), 3);
        for filter in [" fox ", "SWE", "only when you ask", "zzz", ""] {
            assert_eq!(
                filtered_cards(&everything, filter),
                automation_cards(&kraddies, filter, &clock),
                "{filter}"
            );
        }
    }

    #[test]
    fn a_resting_owner_rests_its_automations_and_the_latest_run_sets_the_tone() {
        let mut kraddies = [kraddie("Owl", &["Sweep"])];
        kraddies[0].kraddie.switch = krad_domain::Switch::Off;
        kraddies[0].automations[0].history.start(
            SystemTime::now(),
            KraddieWake::Manual,
            KraddieOutcome::Working,
        );
        let card = &cards(&kraddies, "")[0];
        assert_eq!(card.duty, Duty::OwnerResting);
        assert!(!card.on_duty());
        assert!(card.switched_on());
        assert!(card.running);
        assert_eq!(card.last.map(|last| last.tone), Some(Tone::Live));
        assert_eq!(
            tally(std::slice::from_ref(card)),
            Tally {
                on_duty: 0,
                resting: 1,
                running: 1
            }
        );
    }

    #[test]
    fn a_new_automation_belongs_to_the_kraddie_of_this_repository_or_the_first_one() {
        let mut kraddies = [kraddie("Owl", &[]), kraddie("Fox", &[])];
        let here = KraddieWorkspace {
            environment: krad_domain::EnvironmentName::new("main".to_owned()),
            repo: std::path::PathBuf::from("/work/api"),
        };
        assert_eq!(
            owner_for(&kraddies, Some(&here)),
            Some(kraddies[0].kraddie.id())
        );
        kraddies[1].kraddie.workspace = Some(here.clone());
        assert_eq!(
            owner_for(&kraddies, Some(&here)),
            Some(kraddies[1].kraddie.id())
        );
        assert_eq!(owner_for(&[], Some(&here)), None);
    }

    #[test]
    fn signed_out_hosts_are_locked_and_every_shelf_offers_choices() {
        let draft = AutomationDraft::default();
        let root = cue_root(&draft, CodeHostLogins::default());
        let locked: Vec<bool> = root
            .iter()
            .map(|entry| matches!(entry, CueEntry::Shelf { locked: true, .. }))
            .collect();
        assert_eq!(locked, [false, true, true, false, false]);
        let open = cue_root(
            &draft,
            CodeHostLogins {
                github: true,
                gitlab: true,
            },
        );
        assert!(open
            .iter()
            .all(|entry| !matches!(entry, CueEntry::Shelf { locked: true, .. })));
        assert_eq!(
            cue_shelf(CueShelf::Clock, &draft).len(),
            PRESET_MINUTES.len() + 3
        );
        assert_eq!(
            cue_shelf(CueShelf::Host(CodeHost::Github), &draft).len(),
            CodeHostTrigger::ALL.len()
        );
        assert_eq!(
            cue_shelf(CueShelf::App, &draft).len(),
            AppTrigger::ALL.len()
        );
    }

    #[test]
    fn a_chosen_event_shows_as_picked() {
        let mut draft = AutomationDraft::default();
        draft.toggle_trigger(Trigger::App {
            event: AppTrigger::GitChanged,
        });
        let picked: Vec<bool> = cue_shelf(CueShelf::App, &draft)
            .iter()
            .map(|entry| matches!(entry, CueEntry::Direct { picked: true, .. }))
            .collect();
        assert_eq!(picked.iter().filter(|picked| **picked).count(), 1);
    }
}
