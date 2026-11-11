mod onboarding;
mod rows;

use gpui_kit::{AppContext as _, ClipboardItem, Context};
use krad_chat::{harness_health, harness_off_path};
use krad_domain::{
    parse_wsl_path, AgentPreset, HarnessHealth, HarnessOnboarding, Host, CHECKED_HARNESSES,
};
use krad_host::HostLink;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::chat::StartError;
use super::terminal_card::home_dir;
use super::EnvironmentShell;

pub(super) use rows::health_rows;

const COPIED_TOAST: &str = "Copied";

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum HealthPlace {
    ThisComputer,
    Wsl(String),
    Ssh(Host),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Checked {
    Checking,
    Done(HarnessHealth),
}

#[derive(Default)]
pub(super) struct HarnessChecks {
    checked: HashMap<(HealthPlace, AgentPreset), Checked>,
    onboarding_open: bool,
}

impl HarnessChecks {
    pub(super) const fn onboarding_shown(&self) -> bool {
        self.onboarding_open
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OnboardingStep {
    Wait,
    Skip,
    Show,
}

fn is_ready(harness: AgentPreset, checked: Option<&Checked>) -> bool {
    let (Some(setup), Some(Checked::Done(health))) = (harness.setup(), checked) else {
        return false;
    };
    health.is_ready(setup.minimum)
}

fn onboarding_step<'a>(
    results: impl IntoIterator<Item = (AgentPreset, Option<&'a Checked>)>,
) -> OnboardingStep {
    let results: Vec<(AgentPreset, Option<&Checked>)> = results.into_iter().collect();
    if results
        .iter()
        .any(|&(harness, checked)| is_ready(harness, checked))
    {
        return OnboardingStep::Skip;
    }
    let all_done = results
        .iter()
        .all(|(_, checked)| matches!(checked, Some(Checked::Done(_))));
    if all_done {
        OnboardingStep::Show
    } else {
        OnboardingStep::Wait
    }
}

enum Probe {
    Run(krad_chat::Runner, PathBuf),
    OffPath {
        link: HostLink,
        home: Option<PathBuf>,
        program: String,
    },
    Missing,
}

impl Probe {
    fn run(self, harness: AgentPreset) -> HarnessHealth {
        match self {
            Self::Run(runner, cwd) => harness_health(&runner, &cwd, harness),
            Self::OffPath {
                link,
                home,
                program,
            } => harness_off_path(&link, home.as_deref(), harness, &program),
            Self::Missing => {
                tracing::debug!(
                    harness = harness.display_name(),
                    "harness not found on PATH"
                );
                HarnessHealth::Missing
            }
        }
    }
}

impl EnvironmentShell {
    pub(super) fn health_place(&self) -> (HealthPlace, HostLink, Option<PathBuf>) {
        let link = self.active_link();
        if !link.is_local() {
            return (HealthPlace::Ssh(link.host()), link, None);
        }
        let root = self.active_group().map(|group| group.root.clone());
        match root.as_deref().and_then(parse_wsl_path) {
            Some(folder) => (HealthPlace::Wsl(folder.distro), link, root),
            None => (HealthPlace::ThisComputer, link, None),
        }
    }

    pub(super) fn harness_check(
        &self,
        place: &HealthPlace,
        harness: AgentPreset,
    ) -> Option<&Checked> {
        self.harness_checks.checked.get(&(place.clone(), harness))
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
        self.toast(COPIED_TOAST, cx);
    }

    fn check_harnesses_at(
        &mut self,
        place: &HealthPlace,
        link: &HostLink,
        root: Option<&Path>,
        cx: &mut Context<Self>,
    ) {
        for harness in CHECKED_HARNESSES {
            let key = (place.clone(), harness);
            if self.harness_checks.checked.get(&key) == Some(&Checked::Checking) {
                continue;
            }
            let Some(probe) = self.health_probe(place, link, harness, root) else {
                self.harness_checks.checked.remove(&key);
                continue;
            };
            self.harness_checks
                .checked
                .insert(key.clone(), Checked::Checking);
            let checking = cx.background_spawn(async move { probe.run(harness) });
            cx.spawn(async move |this, cx| {
                let health = checking.await;
                this.update(cx, |shell, cx| {
                    shell
                        .harness_checks
                        .checked
                        .insert(key, Checked::Done(health));
                    shell.advance_harness_onboarding(cx);
                    cx.notify();
                })
                .unwrap_or_default();
            })
            .detach();
        }
        cx.notify();
    }

    fn health_probe(
        &self,
        place: &HealthPlace,
        link: &HostLink,
        harness: AgentPreset,
        root: Option<&Path>,
    ) -> Option<Probe> {
        let absent_in_wsl = matches!(place, HealthPlace::Wsl(_))
            && !self.agents_for(&self.active, root).contains(&harness);
        if absent_in_wsl {
            return Some(Probe::Missing);
        }
        match self.harness_runner(link, harness, root) {
            Ok((runner, cwd)) => Some(Probe::Run(runner, cwd)),
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
                Some(Probe::OffPath {
                    link: link.clone(),
                    home,
                    program,
                })
            }
        }
    }

    fn advance_harness_onboarding(&mut self, cx: &mut Context<Self>) {
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
    use krad_domain::{HarnessVersion, SignIn};

    fn found(major: u32, minor: u32, patch: u32, sign_in: SignIn) -> Checked {
        Checked::Done(HarnessHealth::Found {
            version: Some(HarnessVersion::new(major, minor, patch)),
            sign_in,
        })
    }

    #[test]
    fn onboarding_is_skipped_once_one_harness_is_ready() {
        let ready = found(2, 1, 319, SignIn::SignedIn);
        let step = onboarding_step([
            (AgentPreset::ClaudeCode, Some(&ready)),
            (AgentPreset::Codex, Some(&Checked::Checking)),
            (AgentPreset::Opencode, None),
        ]);
        assert_eq!(step, OnboardingStep::Skip);
    }

    #[test]
    fn onboarding_waits_for_every_check() {
        let missing = Checked::Done(HarnessHealth::Missing);
        let step = onboarding_step([
            (AgentPreset::ClaudeCode, Some(&missing)),
            (AgentPreset::Codex, Some(&Checked::Checking)),
        ]);
        assert_eq!(step, OnboardingStep::Wait);
    }

    #[test]
    fn onboarding_shows_when_no_harness_is_ready() {
        let missing = Checked::Done(HarnessHealth::Missing);
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
