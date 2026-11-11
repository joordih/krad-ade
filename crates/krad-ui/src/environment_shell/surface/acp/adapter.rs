use gpui_kit::{AppContext as _, Context};
use krad_chat::{
    install_managed_adapter, managed_adapter_program, managed_adapter_update_due, Runner,
};
use krad_domain::{
    app_kept_acp_package, parse_wsl_path, AcpAgent, AcpAgentId, AdapterFound, AdapterStart,
    AdapterWork, AgentPreset, EnvironmentName, ManagedAdapter, NpmPackage, TerminalId,
};
use krad_host::HostLink;
use krad_session::find_program;
use krad_store::acp_adapters_dir;
use std::path::{Path, PathBuf};

use crate::environment_shell::chat::StartError;
use crate::environment_shell::terminal_card::home_dir;
use crate::environment_shell::EnvironmentShell;

use super::super::Backend;

const NPM_PROGRAM: &str = "npm";
const INSTALLING_TEXT: &str = "Installing";
const INSTALLING_TAIL: &str = "for its first chat…";
const NO_NODE_TEXT: &str =
    "needs Node.js to install itself. Install Node.js (with npm), then send the message again.";
const FAILED_TEXT: &str = "could not install itself:";
const KEPT_UNCHECKED: &str = "Installs itself in Krad ADE's folder on first use";
const KEPT_CHECKING: &str = "Checking its adapter…";
const KEPT_INSTALLING: &str = "Installing its adapter…";
const KEPT_READY: &str = "Ready; Krad ADE keeps its adapter up to date";
const KEPT_UPDATING: &str = "Ready; looking for a newer adapter…";
const KEPT_NO_NODE: &str = "Needs Node.js (with npm) to install its adapter";
const KEPT_FAILED: &str = "Install failed:";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct KeptAdapterView {
    pub(in crate::environment_shell) detail: String,
    pub(in crate::environment_shell) retries: bool,
}

pub(in crate::environment_shell) fn kept_adapter_view(adapter: &ManagedAdapter) -> KeptAdapterView {
    let (detail, retries) = match adapter {
        ManagedAdapter::Unchecked => (KEPT_UNCHECKED.to_owned(), false),
        ManagedAdapter::Checking => (KEPT_CHECKING.to_owned(), false),
        ManagedAdapter::Installing => (KEPT_INSTALLING.to_owned(), false),
        ManagedAdapter::Ready {
            updating: false, ..
        } => (KEPT_READY.to_owned(), false),
        ManagedAdapter::Ready { updating: true, .. } => (KEPT_UPDATING.to_owned(), false),
        ManagedAdapter::NoNode => (KEPT_NO_NODE.to_owned(), true),
        ManagedAdapter::Failed(reason) => (format!("{KEPT_FAILED} {reason}"), true),
    };
    KeptAdapterView { detail, retries }
}

struct AdapterJob {
    agent: AcpAgentId,
    package: NpmPackage,
    folder: PathBuf,
    program: String,
}

fn found_adapter(job: &AdapterJob) -> AdapterFound {
    if let Some(program) = managed_adapter_program(&job.folder, job.package) {
        let update_due = managed_adapter_update_due(&job.folder);
        return AdapterFound::Installed {
            program,
            update_due,
        };
    }
    if let Some(program) = find_program(&job.program) {
        return AdapterFound::Installed {
            program,
            update_due: false,
        };
    }
    if find_program(NPM_PROGRAM).is_none() {
        return AdapterFound::NoNode;
    }
    AdapterFound::Missing
}

fn run_adapter_work(job: &AdapterJob) -> Result<PathBuf, String> {
    let npm = find_program(NPM_PROGRAM).ok_or_else(|| NO_NODE_TEXT.to_owned())?;
    install_managed_adapter(&Runner::Native(npm), &job.folder, job.package)
        .map_err(|error| error.to_string())
}

fn keeps_adapter_here(link: &HostLink, root: Option<&Path>) -> bool {
    link.is_local() && root.and_then(parse_wsl_path).is_none()
}

pub(in crate::environment_shell) fn app_kept_adapter_present(agent: &AcpAgent) -> bool {
    let Some(package) = app_kept_acp_package(agent.id) else {
        return false;
    };
    let kept = acp_adapters_dir()
        .map(|dir| dir.join(agent.id.as_str()))
        .and_then(|folder| managed_adapter_program(&folder, package));
    kept.is_some() || find_program(NPM_PROGRAM).is_some()
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn acp_runner(
        &mut self,
        link: &HostLink,
        agent: &AcpAgent,
        root: Option<&Path>,
        cx: &mut Context<Self>,
    ) -> Result<(Runner, PathBuf), StartError> {
        let preset = AgentPreset::Acp(agent.id);
        let kept = app_kept_acp_package(agent.id).filter(|_| keeps_adapter_here(link, root));
        let Some(package) = kept else {
            return self.program_runner(link, preset, agent.program.clone(), root);
        };
        let name = self.settings.acp.harness_name(preset);
        let folder = acp_adapters_dir()
            .map(|dir| dir.join(agent.id.as_str()))
            .ok_or(StartError::NoConfigFolder)?;
        let start = self.acp_adapters.entry(agent.id).or_default().start();
        match start {
            AdapterStart::Launch(program) => {
                let cwd = root
                    .map(Path::to_path_buf)
                    .or_else(|| home_dir().map(PathBuf::from))
                    .unwrap_or_default();
                Ok((Runner::Native(program), cwd))
            }
            AdapterStart::Check => {
                let job = AdapterJob {
                    agent: agent.id,
                    package,
                    folder,
                    program: agent.program.clone(),
                };
                Self::check_adapter(job, cx);
                Err(StartError::Waiting)
            }
            AdapterStart::Wait => Err(StartError::Waiting),
            AdapterStart::NoNode => Err(StartError::Adapter(format!("{name} {NO_NODE_TEXT}"))),
            AdapterStart::Failed(reason) => Err(StartError::Adapter(format!(
                "{name} {FAILED_TEXT} {reason}"
            ))),
        }
    }

    pub(in crate::environment_shell) fn note_adapter_install(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        agent: AcpAgentId,
    ) {
        let installing = self
            .acp_adapters
            .get(&agent)
            .is_some_and(ManagedAdapter::is_installing);
        let name = self.settings.acp.harness_name(AgentPreset::Acp(agent));
        let text = format!("{INSTALLING_TEXT} {name} {INSTALLING_TAIL}");
        let Some(chat) = self
            .envs
            .get_mut(env)
            .and_then(|live| live.chats.get_mut(&id))
        else {
            return;
        };
        if installing {
            chat.problem = Some(text);
        } else if chat.problem.as_deref() == Some(text.as_str()) {
            chat.problem = None;
        }
    }

    pub(in crate::environment_shell) fn acp_program_name(&self, agent: &AcpAgent) -> String {
        match self.acp_adapters.get(&agent.id) {
            Some(ManagedAdapter::Ready { program, .. }) => program.to_string_lossy().into_owned(),
            Some(_) | None => agent.program.clone(),
        }
    }

    pub(in crate::environment_shell) fn kept_adapter(
        &self,
        agent: AcpAgentId,
    ) -> Option<KeptAdapterView> {
        app_kept_acp_package(agent)?;
        let unchecked = ManagedAdapter::default();
        let adapter = self.acp_adapters.get(&agent).unwrap_or(&unchecked);
        Some(kept_adapter_view(adapter))
    }

    pub(in crate::environment_shell) fn retry_acp_adapter(
        &mut self,
        agent: AcpAgentId,
        cx: &mut Context<Self>,
    ) {
        if let Some(adapter) = self.acp_adapters.get_mut(&agent) {
            adapter.retry();
        }
        self.detect_agents(cx);
        self.adapter_moved(agent, cx);
    }

    fn check_adapter(job: AdapterJob, cx: &mut Context<Self>) {
        let checking = cx.background_spawn(async move {
            let found = found_adapter(&job);
            (job, found)
        });
        cx.spawn(async move |this, cx| {
            let (job, found) = checking.await;
            this.update(cx, |shell, cx| {
                let agent = job.agent;
                let work = shell
                    .acp_adapters
                    .entry(job.agent)
                    .or_default()
                    .checked(found);
                if let Some(work) = work {
                    Self::do_adapter_work(job, work, cx);
                }
                shell.adapter_moved(agent, cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn do_adapter_work(job: AdapterJob, work: AdapterWork, cx: &mut Context<Self>) {
        let agent = job.agent;
        tracing::info!(agent = %agent, ?work, "keeping an ACP adapter in the app's folder");
        let working = cx.background_spawn(async move { run_adapter_work(&job) });
        cx.spawn(async move |this, cx| {
            let outcome = working.await;
            if let Err(error) = &outcome {
                tracing::warn!(agent = %agent, ?work, %error, "the ACP adapter was not installed");
            }
            this.update(cx, |shell, cx| {
                if let Some(adapter) = shell.acp_adapters.get_mut(&agent) {
                    adapter.finished(work, outcome);
                }
                shell.adapter_moved(agent, cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn adapter_moved(&mut self, agent: AcpAgentId, cx: &mut Context<Self>) {
        let preset = AgentPreset::Acp(agent);
        let waiting: Vec<(EnvironmentName, TerminalId)> = self
            .envs
            .iter()
            .flat_map(|(env, live)| {
                live.chats
                    .iter()
                    .filter_map(move |(id, chat)| match &chat.backend {
                        Backend::Structured(structured)
                            if structured.harness == preset && structured.live().is_none() =>
                        {
                            Some((env.clone(), *id))
                        }
                        Backend::Structured(_) | Backend::Transcript(_) => None,
                    })
            })
            .collect();
        for (env, id) in waiting {
            self.note_adapter_install(&env, id, agent);
            self.flush_structured(&env, id, cx);
        }
        self.list_acp_models(agent, cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_blocked_adapter_offers_a_retry_and_each_state_says_where_it_is() {
        let view = |adapter: ManagedAdapter| kept_adapter_view(&adapter);
        assert_eq!(
            view(ManagedAdapter::Installing),
            KeptAdapterView {
                detail: KEPT_INSTALLING.to_owned(),
                retries: false
            }
        );
        assert!(view(ManagedAdapter::NoNode).retries);
        assert_eq!(
            view(ManagedAdapter::Failed("EACCES".to_owned())).detail,
            "Install failed: EACCES"
        );
        assert!(
            !view(ManagedAdapter::Ready {
                program: PathBuf::from("claude-agent-acp"),
                updating: false
            })
            .retries
        );
    }

    #[test]
    fn only_this_computer_s_own_folders_keep_an_adapter() {
        assert!(keeps_adapter_here(
            &HostLink::Local,
            Some(Path::new("/work/api"))
        ));
        assert!(keeps_adapter_here(&HostLink::Local, None));
        assert!(!keeps_adapter_here(
            &HostLink::Local,
            Some(Path::new(r"\\wsl$\Ubuntu\home\dev"))
        ));
    }
}
