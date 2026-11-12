mod onboarding;
mod rows;

use gpui_kit::{ClipboardItem, Context};
use krad_domain::{parse_wsl_path, AgentPreset, HarnessOnboarding, ToastKind, CHECKED_HARNESSES};
use krad_host::HostLink;
use krad_protocol::{HarnessCheck, HarnessChecked};
use krad_runtime::HarnessProbe;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::chat::StartError;
use super::terminal_card::home_dir;
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
    pub(super) fn health_place(&self) -> (HealthPlace, HostLink, Option<PathBuf>) {
        let link = self.active_link();
        if !link.is_local() {
            return (HealthPlace::Ssh { host: link.host() }, link, None);
        }
        let root = self.active_group().map(|group| group.root.clone());
        match root.as_deref().and_then(parse_wsl_path) {
            Some(folder) => (
                HealthPlace::Wsl {
                    distro: folder.distro,
                },
                link,
                root,
            ),
            None => (HealthPlace::ThisComputer, link, None),
        }
    }

    pub(super) fn harness_check(
        &self,
        place: &HealthPlace,
        harness: AgentPreset,
    ) -> Option<&HarnessCheck> {
        self.runtime.view.health.0.get(&(place.clone(), harness))
    }

    pub(super) fn check_harness_health(&mut self, cx: &mut Context<Self>) {
        let (place, link, root) = self.health_place();
        self.check_harnesses_at(&place, &link, root.as_deref(), cx);
    }

    pub(super) fn start_harness_onboarding(&mut self, cx: &mut Context<Self>) {
        if !self.settings.harness_onboarding.is_pending() {
            return;
        }
        if !self.settings.gateway.is_empty() {
            self.finish_harness_onboarding(cx);
            return;
        }
        self.check_harnesses_at(&HealthPlace::ThisComputer, &HostLink::Local, None, cx);
    }

    pub(super) fn copy_harness_fix(&mut self, text: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
        self.toast(ToastKind::Success, COPIED_TOAST, cx);
    }

    fn check_harnesses_at(
        &mut self,
        place: &HealthPlace,
        link: &HostLink,
        root: Option<&Path>,
        cx: &mut Context<Self>,
    ) {
        let probes = CHECKED_HARNESSES
            .into_iter()
            .map(|harness| (harness, self.health_probe(place, link, harness, root)))
            .collect();
        self.runtime.handle.check_harnesses(place.clone(), probes);
        cx.notify();
    }

    fn health_probe(
        &self,
        place: &HealthPlace,
        link: &HostLink,
        harness: AgentPreset,
        root: Option<&Path>,
    ) -> Option<HarnessProbe> {
        let absent_in_wsl = matches!(place, HealthPlace::Wsl { .. })
            && !self.agents_for(&self.active, root).contains(&harness);
        if absent_in_wsl {
            return Some(HarnessProbe::Missing);
        }
        match self.harness_runner(link, harness, root) {
            Ok((runner, cwd)) => Some(HarnessProbe::Run(runner, cwd)),
            Err(StartError::Waiting) => None,
            Err(_) => {
                let program = self
                    .settings
                    .launch_line(harness)
                    .and_then(|line| line.split_whitespace().next().map(str::to_owned))?;
                let home = match link {
                    HostLink::Local => home_dir().map(PathBuf::from),
                    HostLink::Remote(ssh) => ssh.home().map(Path::to_path_buf),
                };
                Some(HarnessProbe::OffPath {
                    link: link.clone(),
                    home,
                    program,
                })
            }
        }
    }

    pub(super) fn advance_harness_onboarding(&mut self, cx: &mut Context<Self>) {
        if !self.settings.harness_onboarding.is_pending() {
            return;
        }
        let place = HealthPlace::ThisComputer;
        let step = onboarding_step(
            CHECKED_HARNESSES
                .into_iter()
                .map(|harness| (harness, self.harness_check(&place, harness))),
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
        self.check_harnesses_at(&HealthPlace::ThisComputer, &HostLink::Local, None, cx);
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
