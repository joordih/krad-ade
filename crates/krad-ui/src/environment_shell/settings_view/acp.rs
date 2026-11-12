mod admit;
mod draft;
mod form;
mod package;

use gpui_kit::{
    div, px, AnyElement, AppContext as _, ClipboardItem, Context, IntoElement, ParentElement,
    Styled, Window,
};
use krad_domain::{
    acp_package, parse_wsl_path, AcpAgent, AcpAgentId, AcpPackage, AcpPackageState, AgentPreset,
    ToastKind,
};
use krad_store::delete_acp_secret;

use super::super::surface::KeptAdapterView;
use super::sections::{empty_row, footnote, pane_column, settings_section};
use super::{form_row, panel_card, EnvironmentShell};
use crate::components::{Button, ButtonVariant, IconName};
use crate::theme::{palette, Palette, SPACE_2};
use form::AcpForm;
use package::{package_detail, AcpPackages};

const SECRETS_NOTE: &str = "Secret values are kept in the system keyring. Without one, as under WSL, they go in the config folder, readable only by you.";
const BUILT_IN_TITLE: &str = "Known agents";
const BUILT_IN_HINT: &str = "The ACP adapters of the agents the app knows, found on PATH. Install a missing npm adapter here. They show next to the native ones, with (ACP) after their name; Claude Code's adapter takes the place of Claude Code.";
const CUSTOM_TITLE: &str = "Your agents";
const CUSTOM_HINT: &str = "Any other agent that speaks the Agent Client Protocol, by its command line. Start one from the bus with krad-ade-agents spawn --agent <id>.";
const CUSTOM_EMPTY: &str = "No agents yet.";
const DETECTED_DETAIL: &str = "Found on PATH";
const MISSING_DETAIL: &str = "Not found";
const INSTALL_DETAIL: &str = "install it with";
const IN_WSL_DETAIL: &str = "in WSL";
const ON_HOST_DETAIL: &str = "on";
const INSTALL_SEPARATOR: &str = "; ";
const ADD_AGENT_LABEL: &str = "Add agent";
const EDIT_LABEL: &str = "Edit";
const REMOVE_LABEL: &str = "Remove";
const COPY_TOOLTIP: &str = "Copy the install command";
const INSTALL_LABEL: &str = "Install";
const RETRY_LABEL: &str = "Try again";
const INSTALL_TOOLTIP: &str = "Install the agent's npm package globally";
const KEPT_RETRY_TOOLTIP: &str = "Install the adapter in Krad ADE's own folder again";
const INSTALL_PREFIX: &str = "acp-install";
const COPIED_TOAST: &str = "Install command copied";
const REMOVED_TOAST: &str = "ACP agent removed";
const BTN_OPEN_FORM: &str = "acp-open-form";
const COPY_PREFIX: &str = "acp-copy-install";
const EDIT_PREFIX: &str = "acp-agent-edit";
const REMOVE_PREFIX: &str = "acp-agent-remove";
const DETAIL_SEPARATOR: &str = " · ";

pub(in crate::environment_shell) struct AcpUi {
    form: AcpForm,
    packages: AcpPackages,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum DetectionPlace {
    ThisComputer,
    Wsl(String),
    Ssh(String),
}

impl DetectionPlace {
    fn suffix(&self) -> String {
        match self {
            Self::ThisComputer => String::new(),
            Self::Wsl(distro) => format!(" {IN_WSL_DETAIL} {distro}"),
            Self::Ssh(host) => format!(" {ON_HOST_DETAIL} {host}"),
        }
    }
}

fn detection_detail(place: &DetectionPlace, detected: bool, install: Option<&str>) -> String {
    let suffix = place.suffix();
    if detected {
        return format!("{DETECTED_DETAIL}{suffix}");
    }
    match install {
        Some(command) => {
            format!("{MISSING_DETAIL}{suffix}{INSTALL_SEPARATOR}{INSTALL_DETAIL} {command}")
        }
        None => format!("{MISSING_DETAIL}{suffix}"),
    }
}

fn element_index(index: usize) -> u64 {
    u64::try_from(index).unwrap_or(u64::MAX)
}

fn agent_summary(agent: &AcpAgent) -> String {
    let command_line = std::iter::once(agent.program.as_str())
        .chain(agent.args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ");
    format!("{}{DETAIL_SEPARATOR}{command_line}", agent.id)
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn acp_ui(
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AcpUi {
        AcpUi {
            form: AcpForm::new(window, cx),
            packages: AcpPackages::default(),
        }
    }

    fn copy_acp_install(&mut self, command: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(command.to_owned()));
        self.toast(ToastKind::Success, COPIED_TOAST, cx);
    }

    fn edit_acp_agent(&mut self, id: AcpAgentId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(agent) = self
            .settings
            .acp
            .agents
            .iter()
            .find(|agent| agent.id == id)
            .cloned()
        else {
            return;
        };
        self.open_acp_form(Some(&agent), window, cx);
    }

    fn remove_acp_agent(&mut self, id: AcpAgentId, cx: &mut Context<Self>) {
        let Some(agent) = self.settings.acp.remove_agent(id) else {
            return;
        };
        self.save_settings(cx);
        cx.notify();
        let removed = cx.background_spawn(async move {
            agent.secret_keys().iter().try_for_each(delete_acp_secret)
        });
        cx.spawn(async move |this, cx| {
            let removed = removed.await;
            this.update(cx, |shell, cx| match removed {
                Ok(()) => shell.toast(ToastKind::Success, REMOVED_TOAST, cx),
                Err(error) => shell.toast(ToastKind::Error, &error.to_string(), cx),
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn acp_detection_place(&self) -> DetectionPlace {
        if let Some(host) = self
            .envs
            .get(&self.active)
            .map(|live| &live.data.host)
            .filter(|host| !host.is_local())
        {
            return DetectionPlace::Ssh(host.to_string());
        }
        self.active_group()
            .and_then(|group| parse_wsl_path(&group.root))
            .map_or(DetectionPlace::ThisComputer, |folder| {
                DetectionPlace::Wsl(folder.distro)
            })
    }

    fn acp_built_in_row(
        &self,
        index: usize,
        agent: &AcpAgent,
        place: &DetectionPlace,
        packages: &AcpPackages,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        if let Some(kept) = self
            .kept_adapter(agent.id)
            .filter(|_| *place == DetectionPlace::ThisComputer)
        {
            return Self::kept_adapter_row(index, agent, &kept, cx);
        }
        let detected = self
            .installed_agents()
            .contains(&AgentPreset::Acp(agent.id));
        let package = acp_package(agent.id);
        let install = package.map(|package| package.to_string());
        let shown_install = install.as_deref().filter(|_| !detected);
        let npm = package.and_then(AcpPackage::npm);
        let state = npm.map(|_| packages.state(&(place.clone(), agent.id)));
        let detail = match (npm, state) {
            (Some(npm), Some(state)) => format!(
                "{}{DETAIL_SEPARATOR}{}",
                detection_detail(place, detected, None),
                package_detail(npm, state)
            ),
            _ => detection_detail(place, detected, shown_install),
        };
        let copy = install.filter(|_| !detected).map(|command| {
            Button::new((COPY_PREFIX, element_index(index)))
                .icon(IconName::Copy)
                .small()
                .variant(ButtonVariant::Secondary)
                .tooltip(COPY_TOOLTIP)
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.copy_acp_install(&command, cx);
                }))
        });
        let id = agent.id;
        let offer = state.filter(|state| state.can_install()).map(|state| {
            let label = if matches!(state, AcpPackageState::Failed(_)) {
                RETRY_LABEL
            } else {
                INSTALL_LABEL
            };
            Button::new((INSTALL_PREFIX, element_index(index)))
                .label(label)
                .icon(IconName::Add)
                .small()
                .variant(ButtonVariant::Primary)
                .tooltip(INSTALL_TOOLTIP)
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.install_acp_agent(id, cx);
                }))
        });
        let control = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .children(offer)
            .children(copy)
            .into_any_element();
        form_row(&agent.display_name(), Some(&detail), control, pal)
    }

    fn kept_adapter_row(
        index: usize,
        agent: &AcpAgent,
        kept: &KeptAdapterView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let id = agent.id;
        let retry = kept.retries.then(|| {
            Button::new((INSTALL_PREFIX, element_index(index)))
                .label(RETRY_LABEL)
                .icon(IconName::Add)
                .small()
                .variant(ButtonVariant::Primary)
                .tooltip(KEPT_RETRY_TOOLTIP)
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.retry_acp_adapter(id, cx);
                }))
        });
        let control = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .children(retry)
            .into_any_element();
        form_row(&agent.display_name(), Some(&kept.detail), control, pal)
    }

    fn acp_custom_row(
        index: usize,
        agent: &AcpAgent,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = agent.id;
        let controls = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .child(
                Button::new((EDIT_PREFIX, element_index(index)))
                    .label(EDIT_LABEL)
                    .small()
                    .variant(ButtonVariant::Secondary)
                    .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                        this.edit_acp_agent(id, window, cx);
                    })),
            )
            .child(
                Button::new((REMOVE_PREFIX, element_index(index)))
                    .label(REMOVE_LABEL)
                    .small()
                    .variant(ButtonVariant::Secondary)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.remove_acp_agent(id, cx);
                    })),
            )
            .into_any_element();
        form_row(
            &agent.display_name(),
            Some(&agent_summary(agent)),
            controls,
            pal,
        )
    }

    fn acp_built_in_card(&self, packages: &AcpPackages, cx: &mut Context<Self>) -> AnyElement {
        let place = self.acp_detection_place();
        let rows: Vec<AnyElement> = krad_domain::built_in_acp_agents()
            .iter()
            .enumerate()
            .map(|(index, agent)| self.acp_built_in_row(index, agent, &place, packages, cx))
            .collect();
        let pal = palette(cx);
        settings_section(
            BUILT_IN_TITLE,
            Some(BUILT_IN_HINT),
            [panel_card(rows, pal)],
            pal,
        )
    }

    fn acp_custom_card(&self, form_open: bool, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let rows: Vec<AnyElement> = if self.settings.acp.agents.is_empty() {
            vec![empty_row(CUSTOM_EMPTY, pal)]
        } else {
            self.settings
                .acp
                .agents
                .iter()
                .enumerate()
                .map(|(index, agent)| Self::acp_custom_row(index, agent, pal, cx))
                .collect()
        };
        let add = (!form_open).then(|| {
            div()
                .flex()
                .flex_row()
                .justify_end()
                .child(
                    Button::new(BTN_OPEN_FORM)
                        .label(ADD_AGENT_LABEL)
                        .icon(IconName::Add)
                        .small()
                        .variant(ButtonVariant::Secondary)
                        .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                            this.open_acp_form(None, window, cx);
                        })),
                )
                .into_any_element()
        });
        settings_section(
            CUSTOM_TITLE,
            Some(CUSTOM_HINT),
            std::iter::once(panel_card(rows, pal)).chain(add),
            pal,
        )
    }

    pub(super) fn acp_pane(&self, ui: &AcpUi, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let open = ui.form.open;
        let mut sections = vec![
            self.acp_built_in_card(&ui.packages, cx),
            self.acp_custom_card(open, cx),
        ];
        if open {
            sections.push(Self::acp_form_section(&ui.form, cx));
        }
        sections.push(footnote(SECRETS_NOTE, pal));
        pane_column(sections)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_names_the_environment_s_host() {
        let install = Some("npm i -g codex-acp");
        assert_eq!(
            detection_detail(&DetectionPlace::ThisComputer, true, None),
            "Found on PATH"
        );
        assert_eq!(
            detection_detail(&DetectionPlace::Wsl("Ubuntu".to_owned()), true, None),
            "Found on PATH in WSL Ubuntu"
        );
        assert_eq!(
            detection_detail(&DetectionPlace::Ssh("dev@box".to_owned()), false, install),
            "Not found on dev@box; install it with npm i -g codex-acp"
        );
        assert_eq!(
            detection_detail(&DetectionPlace::ThisComputer, false, install),
            "Not found; install it with npm i -g codex-acp"
        );
    }
}
