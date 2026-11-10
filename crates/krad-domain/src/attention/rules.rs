use super::queue::{AttentionKind, WindowFocus};
use super::quiet_hours::{QuietHours, TimeOfDay};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Switch {
    On,
    Off,
}

impl Switch {
    pub const ALL: [Self; 2] = [Self::On, Self::Off];

    #[must_use]
    pub const fn is_on(self) -> bool {
        matches!(self, Self::On)
    }

    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            Self::On => Self::Off,
            Self::Off => Self::On,
        }
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::On => "On",
            Self::Off => "Off",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AttentionSound {
    #[default]
    Silent,
    Chime,
    Ping,
    Pop,
    Bell,
}

impl AttentionSound {
    pub const ALL: [Self; 5] = [Self::Silent, Self::Chime, Self::Ping, Self::Pop, Self::Bell];

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Silent => "None",
            Self::Chime => "Chime",
            Self::Ping => "Ping",
            Self::Pop => "Pop",
            Self::Bell => "Bell",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SoundTiming {
    #[default]
    Background,
    Always,
}

impl SoundTiming {
    pub const ALL: [Self; 2] = [Self::Background, Self::Always];

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Background => "In the background",
            Self::Always => "Always",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionAttention {
    #[default]
    Notify,
    Muted,
}

impl SessionAttention {
    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            Self::Notify => Self::Muted,
            Self::Muted => Self::Notify,
        }
    }

    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct KindRule {
    pub notify: Switch,
    pub sound: AttentionSound,
    pub badge: Switch,
}

pub const DEFAULT_KIND_RULE: KindRule = KindRule {
    notify: Switch::On,
    sound: AttentionSound::Silent,
    badge: Switch::On,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct AttentionRules {
    pub finished: KindRule,
    pub asks: KindRule,
    pub permissions: KindRule,
    pub errors: KindRule,
}

impl Default for AttentionRules {
    fn default() -> Self {
        Self {
            finished: DEFAULT_KIND_RULE,
            asks: DEFAULT_KIND_RULE,
            permissions: DEFAULT_KIND_RULE,
            errors: DEFAULT_KIND_RULE,
        }
    }
}

impl AttentionRules {
    #[must_use]
    pub const fn of(&self, kind: AttentionKind) -> KindRule {
        match kind {
            AttentionKind::Finished => self.finished,
            AttentionKind::Ask => self.asks,
            AttentionKind::Permission => self.permissions,
            AttentionKind::Error => self.errors,
        }
    }

    pub fn of_mut(&mut self, kind: AttentionKind) -> &mut KindRule {
        match kind {
            AttentionKind::Finished => &mut self.finished,
            AttentionKind::Ask => &mut self.asks,
            AttentionKind::Permission => &mut self.permissions,
            AttentionKind::Error => &mut self.errors,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Circumstances {
    pub window: WindowFocus,
    pub session: SessionAttention,
    pub now: Option<TimeOfDay>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct AttentionSettings {
    pub notifications: Switch,
    pub actions: Switch,
    pub badge: Switch,
    pub sound_timing: SoundTiming,
    pub quiet_hours: QuietHours,
    pub rules: AttentionRules,
}

impl Default for AttentionSettings {
    fn default() -> Self {
        Self {
            notifications: Switch::On,
            actions: Switch::On,
            badge: Switch::On,
            sound_timing: SoundTiming::default(),
            quiet_hours: QuietHours::default(),
            rules: AttentionRules::default(),
        }
    }
}

impl AttentionSettings {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    fn hushed(&self, circumstances: Circumstances) -> bool {
        circumstances.session == SessionAttention::Muted
            || self.quiet_hours.contains(circumstances.now)
    }

    #[must_use]
    pub fn notifies(&self, kind: AttentionKind, circumstances: Circumstances) -> bool {
        self.notifications.is_on()
            && self.rules.of(kind).notify.is_on()
            && circumstances.window == WindowFocus::Background
            && !self.hushed(circumstances)
    }

    #[must_use]
    pub fn sound(
        &self,
        kind: AttentionKind,
        circumstances: Circumstances,
    ) -> Option<AttentionSound> {
        let sound = self.rules.of(kind).sound;
        if sound == AttentionSound::Silent || self.hushed(circumstances) {
            return None;
        }
        let heard = match self.sound_timing {
            SoundTiming::Always => true,
            SoundTiming::Background => circumstances.window == WindowFocus::Background,
        };
        heard.then_some(sound)
    }

    #[must_use]
    pub fn counts(&self, kind: AttentionKind, session: SessionAttention) -> bool {
        self.badge.is_on()
            && self.rules.of(kind).badge.is_on()
            && session == SessionAttention::Notify
    }

    #[must_use]
    pub fn offers_answers(&self, kind: AttentionKind) -> bool {
        self.actions.is_on() && kind == AttentionKind::Permission
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AWAY: Circumstances = Circumstances {
        window: WindowFocus::Background,
        session: SessionAttention::Notify,
        now: None,
    };

    fn at(hour: u8) -> Option<TimeOfDay> {
        TimeOfDay::new(hour, 0).ok()
    }

    fn with_sound(kind: AttentionKind, sound: AttentionSound) -> AttentionSettings {
        let mut settings = AttentionSettings::default();
        settings.rules.of_mut(kind).sound = sound;
        settings
    }

    #[test]
    fn every_kind_notifies_by_default_without_a_sound() {
        let settings = AttentionSettings::default();
        for kind in AttentionKind::ALL {
            assert!(settings.notifies(kind, AWAY), "{kind:?}");
            assert_eq!(settings.sound(kind, AWAY), None, "{kind:?}");
            assert!(settings.counts(kind, SessionAttention::Notify), "{kind:?}");
        }
    }

    #[test]
    fn notifications_reach_only_a_background_window() {
        let settings = AttentionSettings::default();
        let here = Circumstances {
            window: WindowFocus::Foreground,
            ..AWAY
        };
        assert!(!settings.notifies(AttentionKind::Ask, here));
    }

    #[test]
    fn a_kind_rule_turns_its_own_notifications_off() {
        let mut settings = AttentionSettings::default();
        settings.rules.of_mut(AttentionKind::Finished).notify = Switch::Off;
        assert!(!settings.notifies(AttentionKind::Finished, AWAY));
        assert!(settings.notifies(AttentionKind::Permission, AWAY));
    }

    #[test]
    fn the_main_switch_silences_every_kind() {
        let settings = AttentionSettings {
            notifications: Switch::Off,
            ..AttentionSettings::default()
        };
        for kind in AttentionKind::ALL {
            assert!(!settings.notifies(kind, AWAY), "{kind:?}");
        }
    }

    #[test]
    fn a_muted_session_neither_notifies_sounds_nor_counts() {
        let settings = with_sound(AttentionKind::Permission, AttentionSound::Ping);
        let muted = Circumstances {
            session: SessionAttention::Muted,
            ..AWAY
        };
        assert!(!settings.notifies(AttentionKind::Permission, muted));
        assert_eq!(settings.sound(AttentionKind::Permission, muted), None);
        assert!(!settings.counts(AttentionKind::Permission, SessionAttention::Muted));
    }

    #[test]
    fn quiet_hours_hush_notifications_and_sounds_but_keep_the_count() {
        let mut settings = with_sound(AttentionKind::Ask, AttentionSound::Chime);
        settings.quiet_hours.switch = Switch::On;
        let night = Circumstances {
            now: at(23),
            ..AWAY
        };
        let day = Circumstances {
            now: at(12),
            ..AWAY
        };
        assert!(!settings.notifies(AttentionKind::Ask, night));
        assert_eq!(settings.sound(AttentionKind::Ask, night), None);
        assert!(settings.counts(AttentionKind::Ask, SessionAttention::Notify));
        assert!(settings.notifies(AttentionKind::Ask, day));
        assert_eq!(
            settings.sound(AttentionKind::Ask, day),
            Some(AttentionSound::Chime)
        );
    }

    #[test]
    fn sounds_follow_their_timing() {
        let mut settings = with_sound(AttentionKind::Error, AttentionSound::Bell);
        let here = Circumstances {
            window: WindowFocus::Foreground,
            ..AWAY
        };
        assert_eq!(settings.sound(AttentionKind::Error, here), None);
        assert_eq!(
            settings.sound(AttentionKind::Error, AWAY),
            Some(AttentionSound::Bell)
        );
        settings.sound_timing = SoundTiming::Always;
        assert_eq!(
            settings.sound(AttentionKind::Error, here),
            Some(AttentionSound::Bell)
        );
    }

    #[test]
    fn the_badge_counts_only_the_kinds_chosen_for_it() {
        let mut settings = AttentionSettings::default();
        settings.rules.of_mut(AttentionKind::Finished).badge = Switch::Off;
        assert!(!settings.counts(AttentionKind::Finished, SessionAttention::Notify));
        assert!(settings.counts(AttentionKind::Ask, SessionAttention::Notify));
        settings.badge = Switch::Off;
        assert!(!settings.counts(AttentionKind::Ask, SessionAttention::Notify));
    }

    #[test]
    fn only_permissions_offer_answers_and_only_when_allowed() {
        let mut settings = AttentionSettings::default();
        assert!(settings.offers_answers(AttentionKind::Permission));
        for kind in [
            AttentionKind::Ask,
            AttentionKind::Error,
            AttentionKind::Finished,
        ] {
            assert!(!settings.offers_answers(kind), "{kind:?}");
        }
        settings.actions = Switch::Off;
        assert!(!settings.offers_answers(AttentionKind::Permission));
    }

    #[test]
    fn settings_read_leniently_from_partial_json() -> Result<(), serde_json::Error> {
        let read: AttentionSettings = serde_json::from_str(
            r#"{"badge":"off","rules":{"errors":{"notify":"off","sound":"bell","badge":"on"}}}"#,
        )?;
        assert_eq!(read.badge, Switch::Off);
        assert_eq!(read.rules.errors.sound, AttentionSound::Bell);
        assert_eq!(read.rules.finished, AttentionRules::default().finished);
        assert_eq!(read.notifications, Switch::On);
        Ok(())
    }
}
