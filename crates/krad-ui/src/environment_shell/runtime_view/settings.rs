use krad_domain::Settings;
use std::collections::VecDeque;
use std::rc::Rc;

#[derive(Default)]
pub(in crate::environment_shell) struct SettingsMirror {
    current: Rc<Settings>,
    sent: VecDeque<Rc<Settings>>,
    previewed: bool,
}

impl SettingsMirror {
    pub(in crate::environment_shell) fn new(settings: Settings) -> Self {
        Self {
            current: Rc::new(settings),
            sent: VecDeque::new(),
            previewed: false,
        }
    }

    pub(in crate::environment_shell) fn current(&self) -> Rc<Settings> {
        Rc::clone(&self.current)
    }

    pub(in crate::environment_shell) fn send(
        &mut self,
        settings: Settings,
    ) -> Option<Rc<Settings>> {
        if *self.current == settings && !self.previewed {
            return None;
        }
        self.previewed = false;
        let sent = Rc::new(settings);
        self.current = Rc::clone(&sent);
        self.sent.push_back(Rc::clone(&sent));
        Some(sent)
    }

    pub(in crate::environment_shell) fn preview(&mut self, settings: Settings) -> Rc<Settings> {
        self.previewed = true;
        self.current = Rc::new(settings);
        self.current()
    }

    fn echoed(&mut self, settings: &Settings) -> bool {
        let Some(sent) = self.sent.iter().position(|sent| **sent == *settings) else {
            return false;
        };
        self.sent.drain(..=sent);
        true
    }

    pub(in crate::environment_shell) fn changed(&mut self, settings: Settings) -> bool {
        if self.echoed(&settings) {
            return false;
        }
        self.sent.clear();
        self.adopt(settings)
    }

    pub(in crate::environment_shell) fn replace(&mut self, settings: Settings) -> bool {
        if self.echoed(&settings) || !self.sent.is_empty() {
            return false;
        }
        self.adopt(settings)
    }

    fn adopt(&mut self, settings: Settings) -> bool {
        if *self.current == settings {
            return false;
        }
        self.current = Rc::new(settings);
        true
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
        assert!(mirror.send(with(AppearanceMode::Dark)).is_some());
        assert!(mirror.send(with(AppearanceMode::Dark)).is_none());
        assert!(mirror.send(with(AppearanceMode::Light)).is_some());
        assert!(!mirror.changed(with(AppearanceMode::Dark)));
        assert!(!mirror.changed(with(AppearanceMode::Light)));
        assert_eq!(mirror.current().appearance, AppearanceMode::Light);
    }

    #[test]
    fn a_change_from_elsewhere_is_adopted_and_known() {
        let mut mirror = SettingsMirror::default();
        assert!(mirror.send(with(AppearanceMode::Dark)).is_some());
        let foreign = with(AppearanceMode::System);
        assert!(mirror.changed(foreign.clone()));
        assert_eq!(*mirror.current(), foreign);
        assert!(mirror.send(foreign).is_none());
    }

    #[test]
    fn a_snapshot_never_overrides_a_save_still_in_flight() {
        let mut mirror = SettingsMirror::default();
        assert!(mirror.send(with(AppearanceMode::Dark)).is_some());
        assert!(!mirror.replace(with(AppearanceMode::Light)));
        assert_eq!(mirror.current().appearance, AppearanceMode::Dark);
        assert!(!mirror.replace(with(AppearanceMode::Dark)));
        assert!(mirror.replace(with(AppearanceMode::Light)));
        assert_eq!(mirror.current().appearance, AppearanceMode::Light);
    }

    #[test]
    fn a_previewed_change_is_sent_when_it_is_saved() {
        let mut mirror = SettingsMirror::default();
        let previewed = mirror.preview(with(AppearanceMode::Dark));
        assert_eq!(previewed.appearance, AppearanceMode::Dark);
        assert!(mirror.send(with(AppearanceMode::Dark)).is_some());
        assert!(mirror.send(with(AppearanceMode::Dark)).is_none());
    }

    #[test]
    fn a_read_shares_the_mirror_s_copy() {
        let mut mirror = SettingsMirror::default();
        let sent = mirror.send(with(AppearanceMode::Dark));
        assert!(sent.is_some_and(|sent| Rc::ptr_eq(&sent, &mirror.current())));
    }
}
