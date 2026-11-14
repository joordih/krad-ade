use krad_chat::{acp_agent_env, Variables};
use krad_domain::{parse_wsl_path, AcpAgentStart, Host, LaunchIntent};
use krad_protocol::{ErrorCode, EventKind, LaunchedAs, Problem, ProblemScope, TerminalLaunched};
use std::collections::BTreeMap;

use super::launching::Resolved;
use super::Runtime;
use crate::hosts::Reach;
use crate::jobs::JobDone;
use crate::launches::WaitingLaunch;
use crate::terminals::{HostShell, TerminalLaunch, TerminalPlace};

const NO_AGENT_TEXT: &str = "That ACP agent is no longer in Settings";
const JOB_NOT_STARTED: &str = "The agent's sign-in could not start";

type Vars = Vec<(String, String)>;

#[derive(Debug)]
pub(crate) struct SignInEnvRead {
    waiting: WaitingLaunch,
    env: Result<Vars, String>,
}

fn sign_in_vars(mut agent_env: Vars, method_env: BTreeMap<String, String>) -> Vars {
    agent_env.retain(|(name, _)| !method_env.contains_key(name));
    agent_env.extend(method_env);
    agent_env
}

impl Runtime {
    pub(super) fn read_sign_in_env(&mut self, waiting: WaitingLaunch, host: &Host) {
        let LaunchIntent::AcpSignIn { agent, env, .. } = &waiting.intent else {
            return;
        };
        let Some(found) = self.settings.current.acp.agent(*agent) else {
            return self.sign_in_failed(&waiting, NO_AGENT_TEXT.to_owned());
        };
        let link = self.hosts.link(host);
        let secrets = self.secrets.clone();
        let method_env = env.clone();
        let terminal = waiting.terminal.clone();
        let started = self.jobs.run(move || {
            let env = secrets
                .read_acp_secrets(&found)
                .map_err(|error| error.to_string())
                .and_then(|secrets| {
                    let start = AcpAgentStart {
                        agent: found,
                        secrets,
                    };
                    acp_agent_env(&start, Variables::from(&link)).map_err(|error| error.to_string())
                })
                .map(|agent_env| sign_in_vars(agent_env, method_env));
            JobDone::SignInEnvRead(SignInEnvRead { waiting, env })
        });
        if let Err(error) = started {
            tracing::warn!(%error, environment = terminal.environment.as_str(), "an ACP sign-in could not start");
            self.outbox.push(EventKind::Problem(Problem {
                scope: ProblemScope::Environment {
                    environment: terminal.environment,
                },
                code: ErrorCode::Failed,
                message: JOB_NOT_STARTED.to_owned(),
            }));
        }
    }

    pub(super) fn sign_in_env_read(&mut self, read: SignInEnvRead) {
        let SignInEnvRead { waiting, env } = read;
        match env {
            Ok(vars) => self.open_sign_in_terminal(waiting, vars),
            Err(error) => self.sign_in_failed(&waiting, error),
        }
    }

    fn sign_in_failed(&mut self, waiting: &WaitingLaunch, message: String) {
        self.outbox.push(EventKind::Problem(Problem {
            scope: ProblemScope::Environment {
                environment: waiting.terminal.environment.clone(),
            },
            code: ErrorCode::Failed,
            message,
        }));
    }

    fn open_sign_in_terminal(&mut self, waiting: WaitingLaunch, env: Vars) {
        let LaunchIntent::AcpSignIn { program, args, .. } = waiting.intent else {
            return;
        };
        let environment = &waiting.terminal.environment;
        let Some(host) = self.environments.host_of(environment) else {
            return;
        };
        let remote = match self.hosts.reach(environment, &host) {
            Reach::Ready(remote) => remote.cloned(),
            Reach::BusPreparing | Reach::Unreached => None,
        };
        let link = self.hosts.link(&host);
        let in_wsl = waiting
            .cwd
            .as_deref()
            .filter(|_| link.is_local())
            .and_then(parse_wsl_path)
            .is_some();
        let mut vars = self.caller_vars(&waiting.terminal, &host, in_wsl, remote.as_ref());
        vars.extend(env);
        let launch = TerminalLaunch {
            program,
            args,
            cwd: waiting.cwd,
            vars,
            place: TerminalPlace::Host {
                link,
                shell: HostShell::Given,
                in_wsl,
            },
            harness: None,
            prefill: waiting.prefill,
            typed: None,
            tap: None,
        };
        let launched = TerminalLaunched {
            terminal: waiting.terminal.clone(),
            harness: None,
            launched_as: LaunchedAs::Line,
            resume: None,
        };
        self.open_resolved(waiting.terminal, Resolved::bare(launch, launched));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_method_s_env_wins_over_the_agent_s_own() {
        let agent_env = vec![
            ("API_KEY".to_owned(), "from-settings".to_owned()),
            ("MODE".to_owned(), "agent".to_owned()),
        ];
        let method_env = BTreeMap::from([("MODE".to_owned(), "login".to_owned())]);
        assert_eq!(
            sign_in_vars(agent_env, method_env),
            [
                ("API_KEY".to_owned(), "from-settings".to_owned()),
                ("MODE".to_owned(), "login".to_owned()),
            ]
        );
    }
}
