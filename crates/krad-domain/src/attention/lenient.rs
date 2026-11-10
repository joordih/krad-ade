use super::quiet_hours::QuietHours;
use super::rules::{AttentionRules, AttentionSettings, KindRule, DEFAULT_KIND_RULE};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use serde_json::Value;

fn field<T: DeserializeOwned>(stored: Option<Value>, fallback: T) -> T {
    stored
        .and_then(|value| T::deserialize(value).ok())
        .unwrap_or(fallback)
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct StoredKindRule {
    notify: Option<Value>,
    sound: Option<Value>,
    badge: Option<Value>,
}

impl<'de> Deserialize<'de> for KindRule {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let stored = StoredKindRule::deserialize(deserializer)?;
        Ok(Self {
            notify: field(stored.notify, DEFAULT_KIND_RULE.notify),
            sound: field(stored.sound, DEFAULT_KIND_RULE.sound),
            badge: field(stored.badge, DEFAULT_KIND_RULE.badge),
        })
    }
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct StoredRules {
    finished: Option<Value>,
    asks: Option<Value>,
    permissions: Option<Value>,
    errors: Option<Value>,
}

impl<'de> Deserialize<'de> for AttentionRules {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let stored = StoredRules::deserialize(deserializer)?;
        Ok(Self {
            finished: field(stored.finished, DEFAULT_KIND_RULE),
            asks: field(stored.asks, DEFAULT_KIND_RULE),
            permissions: field(stored.permissions, DEFAULT_KIND_RULE),
            errors: field(stored.errors, DEFAULT_KIND_RULE),
        })
    }
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct StoredQuietHours {
    switch: Option<Value>,
    start: Option<Value>,
    end: Option<Value>,
}

impl<'de> Deserialize<'de> for QuietHours {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let stored = StoredQuietHours::deserialize(deserializer)?;
        let fallback = Self::default();
        Ok(Self {
            switch: field(stored.switch, fallback.switch),
            start: field(stored.start, fallback.start),
            end: field(stored.end, fallback.end),
        })
    }
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct StoredAttention {
    notifications: Option<Value>,
    actions: Option<Value>,
    badge: Option<Value>,
    sound_timing: Option<Value>,
    quiet_hours: Option<Value>,
    rules: Option<Value>,
}

impl<'de> Deserialize<'de> for AttentionSettings {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let stored = StoredAttention::deserialize(deserializer)?;
        let fallback = Self::default();
        Ok(Self {
            notifications: field(stored.notifications, fallback.notifications),
            actions: field(stored.actions, fallback.actions),
            badge: field(stored.badge, fallback.badge),
            sound_timing: field(stored.sound_timing, fallback.sound_timing),
            quiet_hours: field(stored.quiet_hours, fallback.quiet_hours),
            rules: field(stored.rules, fallback.rules),
        })
    }
}

pub(crate) fn lenient_attention<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<AttentionSettings, D::Error> {
    let stored = Value::deserialize(deserializer)?;
    Ok(field(Some(stored), AttentionSettings::default()))
}

#[cfg(test)]
mod tests {
    use super::super::quiet_hours::TimeOfDay;
    use super::super::rules::{AttentionSound, SoundTiming, Switch};
    use super::*;

    #[test]
    fn one_bad_value_falls_back_for_that_field_only() -> serde_json::Result<()> {
        let read: AttentionSettings = serde_json::from_str(
            r#"{
                "notifications": "off",
                "badge": "sideways",
                "sound_timing": "always",
                "quiet_hours": {"switch": "on", "start": "25:00", "end": "06:30"},
                "rules": {
                    "finished": {"notify": "off", "sound": "kazoo", "badge": "off"},
                    "asks": "loud",
                    "errors": {"sound": "bell"}
                }
            }"#,
        )?;
        let defaults = AttentionSettings::default();
        assert_eq!(read.notifications, Switch::Off);
        assert_eq!(read.badge, defaults.badge);
        assert_eq!(read.sound_timing, SoundTiming::Always);
        assert_eq!(read.quiet_hours.switch, Switch::On);
        assert_eq!(read.quiet_hours.start, defaults.quiet_hours.start);
        assert_eq!(
            Some(read.quiet_hours.end),
            "06:30".parse::<TimeOfDay>().ok()
        );
        assert_eq!(read.rules.finished.notify, Switch::Off);
        assert_eq!(read.rules.finished.sound, DEFAULT_KIND_RULE.sound);
        assert_eq!(read.rules.finished.badge, Switch::Off);
        assert_eq!(read.rules.asks, DEFAULT_KIND_RULE);
        assert_eq!(read.rules.errors.sound, AttentionSound::Bell);
        assert_eq!(read.rules.errors.notify, DEFAULT_KIND_RULE.notify);
        Ok(())
    }

    #[test]
    fn what_was_saved_reads_back_as_itself() -> serde_json::Result<()> {
        let mut settings = AttentionSettings {
            badge: Switch::Off,
            ..AttentionSettings::default()
        };
        settings.rules.permissions.sound = AttentionSound::Pop;
        settings.quiet_hours.switch = Switch::On;
        let back: AttentionSettings = serde_json::from_str(&serde_json::to_string(&settings)?)?;
        assert_eq!(back, settings);
        Ok(())
    }
}
