use gpui_kit::{AppContext as _, Context};
use krad_chat::{acp_package_state, install_acp_package, Runner};
use krad_domain::{
    acp_package, built_in_acp_agents, AcpAgentId, AcpPackage, AcpPackageState, NpmPackage,
    ToastKind,
};
use krad_host::HostLink;
use krad_session::find_program;
use std::collections::HashMap;
use std::path::PathBuf;

use super::super::super::terminal_card::home_dir;
use super::super::super::EnvironmentShell;
use super::DetectionPlace;

const NPM_PROGRAM: &str = "npm";
const NO_NPM_TEXT: &str = "npm was not found; install Node.js to install this agent";
const NO_HOME_TEXT: &str = "the host's home folder is not known yet";
const INSTALLED_TOAST: &str = "installed";
const CHECKING_DETAIL: &str = "Checking npm";
const INSTALLING_DETAIL: &str = "Installing";
const NOT_INSTALLED_DETAIL: &str = "not installed with npm";
const INSTALLED_DETAIL: &str = "installed with npm";
const FAILED_DETAIL: &str = "Install failed";
const ELLIPSIS: &str = "…";

pub(super) type PackageKey = (DetectionPlace, AcpAgentId);

#[derive(Debug, Default)]
pub(super) struct AcpPackages {
    states: HashMap<PackageKey, AcpPackageState>,
}

impl AcpPackages {
    pub(super) fn state(&self, key: &PackageKey) -> &AcpPackageState {
        self.states.get(key).unwrap_or(&AcpPackageState::Unchecked)
    }
}

pub(super) fn package_detail(package: NpmPackage, state: &AcpPackageState) -> String {
    match state {
        AcpPackageState::Unchecked | AcpPackageState::Checking => {
            format!("{CHECKING_DETAIL}{ELLIPSIS}")
        }
        AcpPackageState::Missing => format!("{} {NOT_INSTALLED_DETAIL}", package.name),
        AcpPackageState::Installing => {
            format!("{INSTALLING_DETAIL} {}{ELLIPSIS}", package.name)
        }
        AcpPackageState::Ready {
            version: Some(version),
        } => {
            format!("{} {version} {INSTALLED_DETAIL}", package.name)
        }
        AcpPackageState::Ready { version: None } => {
            format!("{} {INSTALLED_DETAIL}", package.name)
        }
        AcpPackageState::Failed(reason) => format!("{FAILED_DETAIL}: {reason}"),
    }
}

impl EnvironmentShell {
    fn acp_packages(&mut self) -> Option<&mut AcpPackages> {
        self.settings_ui.as_mut().map(|ui| &mut ui.acp.packages)
    }

    fn set_acp_package(&mut self, key: PackageKey, state: AcpPackageState) {
        if let Some(packages) = self.acp_packages() {
            packages.states.insert(key, state);
        }
    }

    fn npm_runner(&self, place: &DetectionPlace) -> Result<(Runner, PathBuf), String> {
        let link = self.active_link();
        if let HostLink::Remote(ssh) = &link {
            let home = ssh.home().map(PathBuf::from).ok_or(NO_HOME_TEXT)?;
            let runner = Runner::Ssh {
                link: link.clone(),
                program: NPM_PROGRAM.to_owned(),
            };
            return Ok((runner, home));
        }
        if let DetectionPlace::Wsl(distro) = place {
            let root = self
                .active_group()
                .map_or_else(PathBuf::new, |group| group.root.clone());
            let runner = Runner::Wsl {
                distro: distro.clone(),
                program: NPM_PROGRAM.to_owned(),
            };
            return Ok((runner, root));
        }
        let npm = find_program(NPM_PROGRAM).ok_or(NO_NPM_TEXT)?;
        let home = home_dir().map_or_else(PathBuf::new, PathBuf::from);
        Ok((Runner::Native(npm), home))
    }

    pub(in crate::environment_shell) fn check_acp_packages(&mut self, cx: &mut Context<Self>) {
        let place = self.acp_detection_place();
        let wanted: Vec<(AcpAgentId, NpmPackage)> = built_in_acp_agents()
            .into_iter()
            .filter_map(|agent| Some((agent.id, acp_package(agent.id)?.npm()?)))
            .collect();
        let runner = self.npm_runner(&place);
        for (id, package) in wanted {
            let key = (place.clone(), id);
            let Some(packages) = self.acp_packages() else {
                return;
            };
            if packages.state(&key).is_busy() {
                continue;
            }
            let (npm, cwd) = match &runner {
                Ok(found) => found.clone(),
                Err(reason) => {
                    self.set_acp_package(key, AcpPackageState::Failed(reason.clone()));
                    continue;
                }
            };
            self.set_acp_package(key.clone(), AcpPackageState::Checking);
            let checking =
                cx.background_spawn(async move { acp_package_state(&npm, &cwd, package) });
            cx.spawn(async move |this, cx| {
                let state = checking
                    .await
                    .unwrap_or_else(|error| AcpPackageState::Failed(error.to_string()));
                this.update(cx, |shell, cx| {
                    shell.set_acp_package(key, state);
                    cx.notify();
                })
                .unwrap_or_default();
            })
            .detach();
        }
        cx.notify();
    }

    pub(super) fn install_acp_agent(&mut self, id: AcpAgentId, cx: &mut Context<Self>) {
        let Some(package) = acp_package(id).and_then(AcpPackage::npm) else {
            return;
        };
        let place = self.acp_detection_place();
        let key = (place.clone(), id);
        let (npm, cwd) = match self.npm_runner(&place) {
            Ok(found) => found,
            Err(reason) => {
                self.set_acp_package(key, AcpPackageState::Failed(reason));
                return cx.notify();
            }
        };
        self.set_acp_package(key.clone(), AcpPackageState::Installing);
        let installing =
            cx.background_spawn(async move { install_acp_package(&npm, &cwd, package) });
        cx.spawn(async move |this, cx| {
            let installed = installing.await;
            this.update(cx, |shell, cx| {
                match installed {
                    Ok(state) => {
                        shell.set_acp_package(key, state);
                        shell.toast(
                            ToastKind::Success,
                            &format!("{} {INSTALLED_TOAST}", package.name),
                            cx,
                        );
                        shell.redetect_acp_agents(cx);
                    }
                    Err(error) => {
                        shell.set_acp_package(key, AcpPackageState::Failed(error.to_string()));
                    }
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
        cx.notify();
    }

    fn redetect_acp_agents(&mut self, cx: &mut Context<Self>) {
        self.detect_agents(cx);
        let remote = self
            .envs
            .get(&self.active)
            .filter(|live| !live.link.is_local())
            .map(|_| self.active.clone());
        if let Some(env) = remote {
            self.detect_host_agents(&env, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADAPTER: NpmPackage = NpmPackage {
        name: "@agentclientprotocol/codex-acp",
        bin: "codex-acp",
    };

    #[test]
    fn each_state_names_the_package_and_a_failure_its_reason() {
        let detail = |state: AcpPackageState| package_detail(ADAPTER, &state);
        assert_eq!(detail(AcpPackageState::Checking), "Checking npm…");
        assert_eq!(
            detail(AcpPackageState::Missing),
            "@agentclientprotocol/codex-acp not installed with npm"
        );
        assert_eq!(
            detail(AcpPackageState::Installing),
            "Installing @agentclientprotocol/codex-acp…"
        );
        assert_eq!(
            detail(AcpPackageState::Ready {
                version: Some("2.6.0".to_owned())
            }),
            "@agentclientprotocol/codex-acp 2.6.0 installed with npm"
        );
        assert_eq!(
            detail(AcpPackageState::Failed("npm error code EACCES".to_owned())),
            "Install failed: npm error code EACCES"
        );
    }
}
