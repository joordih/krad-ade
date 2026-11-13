use krad_domain::Settings;
use std::collections::VecDeque;

#[derive(Default)]
pub(in crate::environment_shell) struct SettingsMirror {
    known: Option<Settings>,
    sent: VecDeque<Settings>,
}

impl SettingsMirror {
    pub(in crate::environment_shell) fn send(&mut self, settings: &Settings) -> bool {
        if self.known.as_ref() == Some(settings) {
            return false;
        }
        self.known = Some(settings.clone());
        self.sent.push_back(settings.clone());
        true
    }

    fn echoed(&mut self, settings: &Settings) -> bool {
        let Some(sent) = self.sent.iter().position(|sent| sent == settings) else {
            return false;
        };
        self.sent.drain(..=sent);
        true
    }

    pub(in crate::environment_shell) fn changed(&mut self, settings: Settings) -> Option<Settings> {
        if self.echoed(&settings) {
            return None;
        }
        self.sent.clear();
        self.known = Some(settings.clone());
        Some(settings)
    }

    pub(in crate::environment_shell) fn replace(&mut self, settings: Settings) {
        if self.echoed(&settings) || !self.sent.is_empty() {
            return;
        }
        self.known = Some(settings);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::AppearanceMode;

    fn with(appearance: AppearanceMode) -> Settings {
        Settings {
            appearance,
            ..Settings::default()
        }
    }

    #[test]
    fn the_runtime_echo_of_a_save_is_not_adopted_again() {
        let mut mirror = SettingsMirror::default();
        assert!(mirror.send(&with(AppearanceMode::Dark)));
        assert!(!mirror.send(&with(AppearanceMode::Dark)));
        assert!(mirror.send(&with(AppearanceMode::Light)));
        assert_eq!(mirror.changed(with(AppearanceMode::Dark)), None);
        assert_eq!(mirror.changed(with(AppearanceMode::Light)), None);
    }

    #[test]
    fn a_change_from_elsewhere_is_adopted_and_known() {
        let mut mirror = SettingsMirror::default();
        assert!(mirror.send(&with(AppearanceMode::Dark)));
        let foreign = with(AppearanceMode::System);
        assert_eq!(mirror.changed(foreign.clone()), Some(foreign.clone()));
        assert!(!mirror.send(&foreign));
    }

    #[test]
    fn a_snapshot_never_overrides_a_save_still_in_flight() {
        let mut mirror = SettingsMirror::default();
        assert!(mirror.send(&with(AppearanceMode::Dark)));
        mirror.replace(with(AppearanceMode::Light));
        assert!(!mirror.send(&with(AppearanceMode::Dark)));
        mirror.replace(with(AppearanceMode::Dark));
        assert!(mirror.send(&with(AppearanceMode::Light)));
    }
}
