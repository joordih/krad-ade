mod tray;

use gpui_kit::{AppContext as _, Context};
use krad_domain::{
    AcpAgent, AcpAuthMethod, AcpAuthMethodId, AcpSignIn, CommandLine, EnvironmentName, NodeKind,
    TerminalId,
};
use std::collections::BTreeMap;

use super::super::super::sessions::Launch;
use super::super::super::terminal_lifecycle::{NodeGroup, NodeSpec};
use super::super::super::EnvironmentShell;
use super::super::structured::read_acp_env;
use super::super::StructuredChat;

const SIGN_IN_TERMINAL_LABEL: &str = "Sign in";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum SignInStep {
    Choosing,
    Signing(AcpAuthMethodId),
    Failed(AcpAuthMethodId),
    InTerminal {
        terminal: TerminalId,
        retry: AcpAuthMethodId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct SignInAsk {
    pub(in crate::environment_shell) methods: Vec<AcpAuthMethod>,
    pub(in crate::environment_shell) step: SignInStep,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SignInAction {
    Authenticate(AcpAuthMethodId),
    Terminal {
        args: Vec<String>,
        env: BTreeMap<String, String>,
        retry: AcpAuthMethodId,
    },
}

impl SignInAsk {
    pub(super) const fn new(methods: Vec<AcpAuthMethod>) -> Self {
        Self {
            methods,
            step: SignInStep::Choosing,
        }
    }

    pub(super) fn asked_again(&mut self, methods: Vec<AcpAuthMethod>) {
        self.methods = methods;
        if let SignInStep::Signing(method) = &self.step {
            self.step = SignInStep::Failed(method.clone());
        }
    }

    fn pick(&mut self, id: &AcpAuthMethodId) -> Option<SignInAction> {
        let method = self.methods.iter().find(|method| &method.id == id)?;
        Some(match &method.sign_in {
            AcpSignIn::ByAgent => {
                self.step = SignInStep::Signing(id.clone());
                SignInAction::Authenticate(id.clone())
            }
            AcpSignIn::InTerminal { args, env } => SignInAction::Terminal {
                args: args.clone(),
                env: env.clone(),
                retry: id.clone(),
            },
        })
    }

    fn failed_method(&self) -> Option<&AcpAuthMethodId> {
        match &self.step {
            SignInStep::Failed(method) => Some(method),
            SignInStep::Choosing | SignInStep::Signing(_) | SignInStep::InTerminal { .. } => None,
        }
    }

    fn terminal_ended(&mut self, ended: TerminalId) -> Option<AcpAuthMethodId> {
        let SignInStep::InTerminal { terminal, retry } = &self.step else {
            return None;
        };
        if *terminal != ended {
            return None;
        }
        let retry = retry.clone();
        self.step = SignInStep::Signing(retry.clone());
        Some(retry)
    }
}

fn sign_in_vars(
    mut agent_env: Vec<(String, String)>,
    method_env: BTreeMap<String, String>,
) -> Vec<(String, String)> {
    agent_env.retain(|(name, _)| !method_env.contains_key(name));
    agent_env.extend(method_env);
    agent_env
}

impl EnvironmentShell {
    fn chat_sign_in(&mut self, id: TerminalId) -> Option<&mut StructuredChat> {
        let env = self.active.clone();
        self.envs
            .get_mut(&env)?
            .chats
            .get_mut(&id)?
            .structured_mut()
    }

    pub(in crate::environment_shell) fn pick_sign_in(
        &mut self,
        id: TerminalId,
        method: &AcpAuthMethodId,
        cx: &mut Context<Self>,
    ) {
        let Some(structured) = self.chat_sign_in(id) else {
            return;
        };
        let Some(action) = structured
            .acp
            .sign_in
            .as_mut()
            .and_then(|asked| asked.pick(method))
        else {
            return;
        };
        match action {
            SignInAction::Authenticate(method) => {
                if let Some(live) = structured.live() {
                    live.sign_in(method);
                }
            }
            SignInAction::Terminal { args, env, retry } => {
                let agent = structured
                    .harness
                    .acp_agent()
                    .and_then(|agent| self.settings.acp.agent(agent));
                if let Some(agent) = agent {
                    let command = CommandLine {
                        program: agent.program.clone(),
                        args,
                    };
                    Self::sign_in_with_agent_env(id, agent, command, env, retry, cx);
                }
            }
        }
        cx.notify();
    }

    pub(in crate::environment_shell) fn sign_in_in_terminal(
        &mut self,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) {
        let Some(structured) = self.chat_sign_in(id) else {
            return;
        };
        let harness = structured.harness;
        let Some(retry) = structured
            .acp
            .sign_in
            .as_ref()
            .and_then(SignInAsk::failed_method)
            .cloned()
        else {
            return;
        };
        let Some(agent) = harness
            .acp_agent()
            .and_then(|agent| self.settings.acp.agent(agent))
        else {
            return;
        };
        if let Some(command) = agent.terminal_command() {
            Self::sign_in_with_agent_env(id, agent, command, BTreeMap::new(), retry, cx);
        }
    }

    fn sign_in_with_agent_env(
        id: TerminalId,
        agent: AcpAgent,
        command: CommandLine,
        method_env: BTreeMap<String, String>,
        retry: AcpAuthMethodId,
        cx: &mut Context<Self>,
    ) {
        let reading = cx.background_spawn(async move { read_acp_env(agent) });
        cx.spawn(async move |this, cx| {
            let agent_env = reading.await;
            this.update(cx, |shell, cx| {
                match agent_env {
                    Ok(agent_env) => {
                        let launch = Launch::Command {
                            program: command.program,
                            args: command.args,
                            vars: sign_in_vars(agent_env, method_env),
                        };
                        shell.open_sign_in_terminal(id, launch, retry, cx);
                    }
                    Err(error) => shell.push_toast(error),
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn open_sign_in_terminal(
        &mut self,
        id: TerminalId,
        launch: Launch,
        retry: AcpAuthMethodId,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        let group = self
            .envs
            .get(&env)
            .and_then(|live| live.data.nodes.iter().find(|node| node.id == id))
            .and_then(|node| node.group);
        let spec = NodeSpec {
            kind: NodeKind::Terminal,
            harness: None,
            label: SIGN_IN_TERMINAL_LABEL,
            group: NodeGroup::OrSelected(group),
        };
        let Some(terminal) = self.open_terminal(&env, &spec, launch, Some(id), cx) else {
            return;
        };
        if let Some(asked) = self
            .chat_sign_in(id)
            .and_then(|structured| structured.acp.sign_in.as_mut())
        {
            asked.step = SignInStep::InTerminal { terminal, retry };
        }
    }

    pub(in crate::environment_shell) fn acp_sign_in_ended(
        &mut self,
        env: &EnvironmentName,
        terminal: TerminalId,
    ) {
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        for chat in live.chats.values_mut() {
            let Some(structured) = chat.structured_mut() else {
                continue;
            };
            let Some(retry) = structured
                .acp
                .sign_in
                .as_mut()
                .and_then(|asked| asked.terminal_ended(terminal))
            else {
                continue;
            };
            if let Some(chat) = structured.live() {
                chat.sign_in(retry);
            }
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn method(id: &str, sign_in: AcpSignIn) -> AcpAuthMethod {
        AcpAuthMethod {
            id: AcpAuthMethodId(id.to_owned()),
            name: id.to_owned(),
            description: None,
            sign_in,
        }
    }

    fn asked() -> SignInAsk {
        SignInAsk::new(vec![
            method("login", AcpSignIn::ByAgent),
            method(
                "cli",
                AcpSignIn::InTerminal {
                    args: vec!["auth".to_owned(), "login".to_owned()],
                    env: BTreeMap::new(),
                },
            ),
        ])
    }

    fn id(raw: &str) -> AcpAuthMethodId {
        AcpAuthMethodId(raw.to_owned())
    }

    #[test]
    fn a_method_the_agent_signs_in_with_authenticates_and_a_new_ask_means_it_failed() {
        let mut asked = asked();
        assert_eq!(
            asked.pick(&id("login")),
            Some(SignInAction::Authenticate(id("login")))
        );
        assert_eq!(asked.step, SignInStep::Signing(id("login")));
        assert_eq!(asked.failed_method(), None);
        let methods = asked.methods.clone();
        asked.asked_again(methods);
        assert_eq!(asked.failed_method(), Some(&id("login")));
        assert_eq!(asked.pick(&id("other")), None);
    }

    #[test]
    fn a_terminal_method_runs_its_args_and_retries_once_its_own_terminal_ends() {
        let mut asked = asked();
        assert!(matches!(
            asked.pick(&id("cli")),
            Some(SignInAction::Terminal { args, retry, .. })
                if args == ["auth", "login"] && retry == id("cli")
        ));
        asked.step = SignInStep::InTerminal {
            terminal: TerminalId(7),
            retry: id("cli"),
        };
        assert_eq!(asked.terminal_ended(TerminalId(8)), None);
        assert_eq!(asked.terminal_ended(TerminalId(7)), Some(id("cli")));
        assert_eq!(asked.step, SignInStep::Signing(id("cli")));
    }

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
