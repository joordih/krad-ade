mod onboarding;
mod rows;

use gpui_kit::{App, ClipboardItem, Context};
use krad_domain::{parse_wsl_path, AgentPreset, HarnessOnboarding, ToastKind, CHECKED_HARNESSES};
use krad_protocol::{Command, HarnessCheck, HarnessChecked, HarnessSpot, HealthCommand};
use std::collections::HashMap;

use super::EnvironmentShell;

pub(super) use krad_protocol::HealthPlace;
pub(super) use rows::health_rows;

const COPIED_TOAST: &str = "Copied";

#[derive(Default)]
pub(super) struct HarnessChecks {
    onboarding_open: bool,
}

impl HarnessChecks {
    pub(super) const fn onboarding_shown(&self) -> bool {
        self.onboarding_open
    }
}

#[derive(Default)]
pub(super) struct HealthMirror(HashMap<(HealthPlace, AgentPreset), HarnessCheck>);

impl HealthMirror {
    pub(super) fn apply(&mut self, checked: HarnessChecked) {
        let key = (checked.place, checked.harness);
        match checked.check {
            HarnessCheck::Forgotten => {
                self.0.remove(&key);
            }
            check @ (HarnessCheck::Checking | HarnessCheck::Done(_)) => {
                self.0.insert(key, check);
            }
        }
    }

    pub(super) fn replace(&mut self, checks: Vec<HarnessChecked>) {
        self.0.clear();
        for checked in checks {
            self.apply(checked);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OnboardingStep {
    Wait,
    Skip,
    Show,
}

fn is_ready(harness: AgentPreset, checked: Option<&HarnessCheck>) -> bool {
    let (Some(setup), Some(HarnessCheck::Done(health))) = (harness.setup(), checked) else {
        return false;
    };
    health.is_ready(setup.minimum)
}

fn onboarding_step<'a>(
    results: impl IntoIterator<Item = (AgentPreset, Option<&'a HarnessCheck>)>,
) -> OnboardingStep {
    let results: Vec<(AgentPreset, Option<&HarnessCheck>)> = results.into_iter().collect();
    if results
        .iter()
        .any(|&(harness, checked)| is_ready(harness, checked))
    {
        return OnboardingStep::Skip;
    }
    let all_done = results
        .iter()
        .all(|(_, checked)| matches!(checked, Some(HarnessCheck::Done(_))));
    if all_done {
        OnboardingStep::Show
    } else {
        OnboardingStep::Wait
    }
}

impl EnvironmentShell {
    pub(super) fn health_place(&self) -> HealthPlace {
        let host = self.active_host();
        if !host.is_local() {
            return HealthPlace::Ssh { host };
        }
        let root = self.active_group().map(|group| group.root.clone());
        match root.as_deref().and_then(parse_wsl_path) {
            Some(folder) => HealthPlace::Wsl {
                distro: folder.distro,
            },
            None => HealthPlace::ThisComputer,
        }
    }

    fn health_spot(&self) -> HarnessSpot {
        let root = self.active_group().map(|group| group.root.clone());
        HarnessSpot::at(self.active.clone(), root)
    }

    pub(super) fn harness_check<'a>(
        &self,
        place: &HealthPlace,
        harness: AgentPreset,
        cx: &'a App,
    ) -> Option<&'a HarnessCheck> {
        self.runtime
            .view
            .read(cx)
            .health
            .0
            .get(&(place.clone(), harness))
    }

    pub(super) fn check_harness_health(&mut self, cx: &mut Context<Self>) {
        let spot = self.health_spot();
        self.check_harnesses_at(spot, cx);
    }

    pub(super) fn start_harness_onboarding(&mut self, cx: &mut Context<Self>) {
        if !self.settings.harness_onboarding.is_pending() {
            return;
        }
        if !self.settings.gateway.is_empty() {
            self.finish_harness_onboarding(cx);
            return;
        }
        self.check_harnesses_at(HarnessSpot::default(), cx);
    }

    pub(super) fn copy_harness_fix(&mut self, text: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
        self.toast(ToastKind::Success, COPIED_TOAST, cx);
    }

    fn check_harnesses_at(&mut self, spot: HarnessSpot, cx: &mut Context<Self>) {
        self.runtime
            .client
            .post(Command::Health(HealthCommand::Check { spot }));
        cx.notify();
    }

    pub(super) fn advance_harness_onboarding(&mut self, cx: &mut Context<Self>) {
        if !self.settings.harness_onboarding.is_pending() {
            return;
        }
        let place = HealthPlace::ThisComputer;
        let step = onboarding_step(
            CHECKED_HARNESSES
                .into_iter()
                .map(|harness| (harness, self.harness_check(&place, harness, cx))),
        );
        match step {
            OnboardingStep::Wait => {}
            OnboardingStep::Skip => self.finish_harness_onboarding(cx),
            OnboardingStep::Show => {
                self.harness_checks.onboarding_open = true;
                cx.notify();
            }
        }
    }

    fn recheck_onboarding(&mut self, cx: &mut Context<Self>) {
        self.check_harnesses_at(HarnessSpot::default(), cx);
    }

    fn finish_harness_onboarding(&mut self, cx: &mut Context<Self>) {
        self.harness_checks.onboarding_open = false;
        self.settings.harness_onboarding = HarnessOnboarding::Done;
        self.save_settings(cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{HarnessHealth, HarnessVersion, SignIn};

    fn found(major: u32, minor: u32, patch: u32, sign_in: SignIn) -> HarnessCheck {
        HarnessCheck::Done(HarnessHealth::Found {
            version: Some(HarnessVersion::new(major, minor, patch)),
            sign_in,
        })
    }

    #[test]
    fn onboarding_is_skipped_once_one_harness_is_ready() {
        let ready = found(2, 1, 319, SignIn::SignedIn);
        let step = onboarding_step([
            (AgentPreset::ClaudeCode, Some(&ready)),
            (AgentPreset::Codex, Some(&HarnessCheck::Checking)),
            (AgentPreset::Opencode, None),
        ]);
        assert_eq!(step, OnboardingStep::Skip);
    }

    #[test]
    fn onboarding_waits_for_every_check() {
        let missing = HarnessCheck::Done(HarnessHealth::Missing);
        let step = onboarding_step([
            (AgentPreset::ClaudeCode, Some(&missing)),
            (AgentPreset::Codex, Some(&HarnessCheck::Checking)),
        ]);
        assert_eq!(step, OnboardingStep::Wait);
    }

    #[test]
    fn onboarding_shows_when_no_harness_is_ready() {
        let missing = HarnessCheck::Done(HarnessHealth::Missing);
        let signed_out = found(0, 166, 0, SignIn::SignedOut);
        let old = found(2, 0, 0, SignIn::Unknown);
        let step = onboarding_step([
            (AgentPreset::ClaudeCode, Some(&old)),
            (AgentPreset::Codex, Some(&signed_out)),
            (AgentPreset::Opencode, Some(&missing)),
        ]);
        assert_eq!(step, OnboardingStep::Show);
    }
}
