use gpui_kit::Context;
use krad_client::ClientError;
use krad_domain::{
    acp_package, built_in_acp_agents, AcpAgentId, AcpPackage, AcpPackageState, NpmPackage,
    ToastKind,
};
use krad_protocol::{AcpPackageShown, Answer, HarnessSpot, HostCommand};
use std::collections::HashMap;

use super::super::super::EnvironmentShell;
use super::DetectionPlace;
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

fn packages_of(answer: Answer) -> Option<Vec<AcpPackageShown>> {
    match answer {
        Answer::AcpPackages { packages } => Some(packages),
        _ => None,
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

    fn acp_spot(&self) -> HarnessSpot {
        let root = self.active_group().map(|group| group.root.clone());
        HarnessSpot::at(self.active.clone(), root)
    }

    fn packages_shown(&mut self, place: &DetectionPlace, shown: Vec<AcpPackageShown>) {
        for AcpPackageShown { agent, state } in shown {
            self.set_acp_package((place.clone(), agent), state);
        }
    }

    pub(in crate::environment_shell) fn check_acp_packages(&mut self, cx: &mut Context<Self>) {
        let place = self.acp_detection_place();
        let wanted: Vec<AcpAgentId> = built_in_acp_agents()
            .into_iter()
            .filter(|agent| acp_package(agent.id).and_then(AcpPackage::npm).is_some())
            .map(|agent| agent.id)
            .filter(|id| {
                self.settings_ui
                    .as_ref()
                    .is_some_and(|ui| !ui.acp.packages.state(&(place.clone(), *id)).is_busy())
            })
            .collect();
        if wanted.is_empty() {
            return;
        }
        for id in &wanted {
            self.set_acp_package((place.clone(), *id), AcpPackageState::Checking);
        }
        let checking = self.ask_host_query(
            HostCommand::AcpPackages {
                spot: self.acp_spot(),
            },
            packages_of,
            cx,
        );
        cx.spawn(async move |this, cx| {
            let checked = checking.await;
            this.update(cx, |shell, cx| {
                match checked {
                    Ok(shown) => shell.packages_shown(&place, shown),
                    Err(error) => {
                        for id in wanted {
                            let failed = AcpPackageState::Failed(error.to_string());
                            shell.set_acp_package((place.clone(), id), failed);
                        }
                    }
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
        cx.notify();
    }

    pub(super) fn install_acp_agent(&mut self, id: AcpAgentId, cx: &mut Context<Self>) {
        let Some(package) = acp_package(id).and_then(AcpPackage::npm) else {
            return;
        };
        let place = self.acp_detection_place();
        let key = (place.clone(), id);
        self.set_acp_package(key.clone(), AcpPackageState::Installing);
        let installing = self.ask_host_query(
            HostCommand::InstallAcpPackage {
                spot: self.acp_spot(),
                agent: id,
            },
            packages_of,
            cx,
        );
        cx.spawn(async move |this, cx| {
            let installed = installing.await;
            this.update(cx, |shell, cx| {
                shell.acp_package_installed(key, package, installed, cx);
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
        cx.notify();
    }

    fn acp_package_installed(
        &mut self,
        key: PackageKey,
        package: NpmPackage,
        installed: Result<Vec<AcpPackageShown>, ClientError>,
        cx: &mut Context<Self>,
    ) {
        let state = match installed {
            Ok(shown) => shown
                .into_iter()
                .find(|shown| shown.agent == key.1)
                .map_or(AcpPackageState::Missing, |shown| shown.state),
            Err(error) => AcpPackageState::Failed(error.to_string()),
        };
        let ready = matches!(state, AcpPackageState::Ready { .. });
        self.set_acp_package(key, state);
        if ready {
            self.toast(
                ToastKind::Success,
                &format!("{} {INSTALLED_TOAST}", package.name),
                cx,
            );
            self.redetect_acp_agents();
        }
    }

    fn redetect_acp_agents(&mut self) {
        self.detect_agents();
        let remote = self
            .envs
            .get(&self.active)
            .filter(|live| !live.data.host.is_local())
            .map(|_| self.active.clone());
        if let Some(env) = remote {
            self.detect_host_agents(&env);
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
