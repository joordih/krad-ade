use gpui_kit::{AnyElement, Context, IntoElement, SharedString};
use krad_domain::{AgentPreset, HarnessHealth, HarnessSetup, HealthIssue, CHECKED_HARNESSES};

use super::super::dialog_parts::sheet_row_with;
use super::super::EnvironmentShell;
use super::{Checked, HealthPlace};
use crate::components::{harness_icon, icon, Button, ButtonVariant, IconName};
use crate::theme::{palette, ICON_LG};

const NOT_CHECKED: &str = "Not checked yet";
const CHECKING: &str = "Checking…";
const READY: &str = "Ready";
const VERSION_WORD: &str = "version";
const UNKNOWN_VERSION: &str = "Couldn't read its version";
const SIGN_IN_UNKNOWN: &str = "Couldn't tell whether it's signed in";
const DETAIL_SEPARATOR: &str = " · ";
const ISSUE_SEPARATOR: &str = ". ";
const COPY_TOOLTIP: &str = "Copy the fix";
const UNKNOWN_FOLDER: &str = "its folder";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct HealthLine {
    pub(super) detail: String,
    pub(super) fix: Option<String>,
}

fn issue_text(issue: &HealthIssue, setup: &HarnessSetup) -> String {
    match issue {
        HealthIssue::NotInstalled => {
            format!("Not found on PATH. Install it with {}", setup.install)
        }
        HealthIssue::NotOnPath { program } => format!(
            "Installed at {} but not on PATH. Add {} to PATH in your shell profile",
            program.display(),
            program
                .parent()
                .map_or_else(|| UNKNOWN_FOLDER.to_owned(), |folder| folder.display().to_string()),
        ),
        HealthIssue::TooOld { found, minimum } => format!(
            "Version {found} is older than {minimum}, the oldest the app supports. Update it with {}",
            setup.update
        ),
        HealthIssue::SignedOut => {
            format!("Not signed in. Sign in from a terminal with {}", setup.sign_in)
        }
        HealthIssue::UnknownVersion => UNKNOWN_VERSION.to_owned(),
        HealthIssue::SignInUnknown => SIGN_IN_UNKNOWN.to_owned(),
    }
}

fn issue_fix(issue: &HealthIssue, setup: &HarnessSetup) -> Option<String> {
    match issue {
        HealthIssue::NotInstalled => Some(setup.install.to_owned()),
        HealthIssue::NotOnPath { program } => {
            program.parent().map(|folder| folder.display().to_string())
        }
        HealthIssue::TooOld { .. } => Some(setup.update.to_owned()),
        HealthIssue::SignedOut => Some(setup.sign_in.to_owned()),
        HealthIssue::UnknownVersion | HealthIssue::SignInUnknown => None,
    }
}

fn ready_detail(health: &HarnessHealth) -> String {
    match health {
        HarnessHealth::Found {
            version: Some(version),
            ..
        } => format!("{READY}{DETAIL_SEPARATOR}{VERSION_WORD} {version}"),
        HarnessHealth::Found { version: None, .. }
        | HarnessHealth::Missing
        | HarnessHealth::OffPath { .. } => READY.to_owned(),
    }
}

pub(super) fn health_line(setup: &HarnessSetup, checked: Option<&Checked>) -> HealthLine {
    let health = match checked {
        None => {
            return HealthLine {
                detail: NOT_CHECKED.to_owned(),
                fix: None,
            }
        }
        Some(Checked::Checking) => {
            return HealthLine {
                detail: CHECKING.to_owned(),
                fix: None,
            }
        }
        Some(Checked::Done(health)) => health,
    };
    let issues = health.issues(setup.minimum);
    if issues.is_empty() {
        return HealthLine {
            detail: ready_detail(health),
            fix: None,
        };
    }
    let detail = issues
        .iter()
        .map(|issue| issue_text(issue, setup))
        .collect::<Vec<_>>()
        .join(ISSUE_SEPARATOR);
    let fix = issues.iter().find_map(|issue| issue_fix(issue, setup));
    HealthLine { detail, fix }
}

fn element_index(index: usize) -> u64 {
    u64::try_from(index).unwrap_or(u64::MAX)
}

pub(in crate::environment_shell) fn health_rows(
    shell: &EnvironmentShell,
    place: &HealthPlace,
    id_prefix: &'static str,
    cx: &mut Context<EnvironmentShell>,
) -> Vec<AnyElement> {
    CHECKED_HARNESSES
        .into_iter()
        .enumerate()
        .filter_map(|(index, harness)| {
            let setup = harness.setup()?;
            let line = health_line(&setup, shell.harness_check(place, harness));
            Some(health_row(index, harness, line, id_prefix, cx))
        })
        .collect()
}

fn health_row(
    index: usize,
    harness: AgentPreset,
    line: HealthLine,
    id_prefix: &'static str,
    cx: &mut Context<EnvironmentShell>,
) -> AnyElement {
    let pal = palette(cx);
    let (mark, color) = harness_icon(harness, pal);
    let control = line.fix.map(|fix| {
        Button::new((id_prefix, element_index(index)))
            .icon(IconName::Copy)
            .small()
            .variant(ButtonVariant::Secondary)
            .tooltip(COPY_TOOLTIP)
            .on_click(cx.listener(move |this: &mut EnvironmentShell, _, _, cx| {
                this.copy_harness_fix(&fix, cx);
            }))
            .into_any_element()
    });
    sheet_row_with(
        Some(icon(mark, ICON_LG, color).into_any_element()),
        harness.display_name(),
        Some(SharedString::from(line.detail)),
        control,
        pal,
    )
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{HarnessVersion, SignIn};
    use std::path::PathBuf;

    fn claude() -> HarnessSetup {
        AgentPreset::ClaudeCode
            .setup()
            .unwrap_or_else(|| unreachable!("Claude Code has a setup"))
    }

    fn line_of(health: HarnessHealth) -> HealthLine {
        health_line(&claude(), Some(&Checked::Done(health)))
    }

    #[test]
    fn a_missing_harness_offers_its_install_command() {
        let line = line_of(HarnessHealth::Missing);
        assert!(line.detail.starts_with("Not found on PATH"));
        assert_eq!(line.fix.as_deref(), Some(claude().install));
    }

    #[test]
    fn a_harness_off_path_offers_its_folder() {
        let line = line_of(HarnessHealth::OffPath {
            program: PathBuf::from("/home/u/.local/bin/claude"),
        });
        assert!(line.detail.contains("/home/u/.local/bin/claude"));
        assert_eq!(line.fix.as_deref(), Some("/home/u/.local/bin"));
    }

    #[test]
    fn an_old_signed_out_harness_offers_the_update_first() {
        let line = line_of(HarnessHealth::Found {
            version: Some(HarnessVersion::new(2, 0, 0)),
            sign_in: SignIn::SignedOut,
        });
        assert!(line.detail.contains("Version 2.0.0 is older than 2.1.41"));
        assert!(line.detail.contains(claude().sign_in));
        assert_eq!(line.fix.as_deref(), Some(claude().update));
    }

    #[test]
    fn a_signed_out_harness_offers_its_sign_in_command() {
        let line = line_of(HarnessHealth::Found {
            version: Some(HarnessVersion::new(2, 1, 319)),
            sign_in: SignIn::SignedOut,
        });
        assert_eq!(line.fix.as_deref(), Some(claude().sign_in));
    }

    #[test]
    fn a_ready_harness_shows_its_version_and_no_fix() {
        let line = line_of(HarnessHealth::Found {
            version: Some(HarnessVersion::new(2, 1, 319)),
            sign_in: SignIn::SignedIn,
        });
        assert_eq!(line.detail, "Ready · version 2.1.319");
        assert_eq!(line.fix, None);
    }

    #[test]
    fn checks_in_flight_or_not_run_say_so() {
        assert_eq!(health_line(&claude(), None).detail, NOT_CHECKED);
        assert_eq!(
            health_line(&claude(), Some(&Checked::Checking)).detail,
            CHECKING
        );
    }
}
