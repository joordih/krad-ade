use futures::channel::oneshot::Sender as ReplySender;
use gpui_kit::Context;
use krad_agents::{terminal_env, Caller, PromptAnswer, Request, Response, ASKPASS_WAIT};
use krad_domain::{parse_wsl_path, EnvironmentName};
use krad_host::mux::{askpass_alias, PromptAsker, SignInPrompt};
use krad_repo::Askpass;
use krad_store::config_dir;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use super::super::EnvironmentShell;

pub(super) const GIT_ONLY: &str = "Only git asks for a sign-in through the bus";
const ANSWER_THREAD: &str = "krad-ade-git-askpass";

pub(in crate::environment_shell) enum AskpassPlan {
    Off,
    Local {
        cli: PathBuf,
        config: Option<PathBuf>,
        bus: Vec<(String, String)>,
    },
    Ready(Askpass),
}

impl AskpassPlan {
    pub(in crate::environment_shell) fn resolve(self) -> Askpass {
        match self {
            Self::Off => Askpass::default(),
            Self::Ready(askpass) => askpass,
            Self::Local { cli, config, bus } => askpass_alias(&cli, config.as_deref()).map_or_else(
                |error| {
                    tracing::debug!(%error, "git runs without an askpass program");
                    Askpass::default()
                },
                |program| Askpass::new(program, bus),
            ),
        }
    }
}

impl EnvironmentShell {
    pub(super) fn answer_git(
        &mut self,
        request: Request,
        reply: ReplySender<Response>,
        cx: &mut Context<Self>,
    ) {
        let Request::Askpass { prompt } = request else {
            reply
                .send(Response::Error(GIT_ONLY.to_owned()))
                .unwrap_or_default();
            return;
        };
        let (answer, answered) = mpsc::channel();
        if self
            .prompt_sender()
            .send(SignInPrompt::new(PromptAsker::Git, prompt, answer))
            .is_err()
        {
            reply.send(Response::Answered(None)).unwrap_or_default();
            return;
        }
        self.collect_sign_in_prompts(cx);
        let waiting = std::thread::Builder::new()
            .name(ANSWER_THREAD.to_owned())
            .spawn(move || {
                let answer = answered.recv_timeout(ASKPASS_WAIT).ok().flatten();
                reply
                    .send(Response::Answered(
                        answer.map(|secret| PromptAnswer::new(secret.expose().to_owned())),
                    ))
                    .unwrap_or_default();
            });
        if let Err(error) = waiting {
            tracing::warn!(%error, "could not wait for a git sign-in answer");
        }
    }

    pub(in crate::environment_shell) fn git_askpass(
        &self,
        env: &EnvironmentName,
        root: &Path,
    ) -> AskpassPlan {
        let (Some(bus), Some(live)) = (&self.agent_bus, self.envs.get(env)) else {
            return AskpassPlan::Off;
        };
        let caller = Caller::Git(env.as_str().to_owned());
        if !live.link.is_local() {
            return live.remote_bus.as_ref().map_or(AskpassPlan::Off, |remote| {
                let (program, vars) = remote.askpass(&caller);
                AskpassPlan::Ready(Askpass::new(program, vars))
            });
        }
        if parse_wsl_path(root).is_some() {
            return AskpassPlan::Off;
        }
        self.host_tools
            .local_cli
            .clone()
            .map_or(AskpassPlan::Off, |cli| AskpassPlan::Local {
                cli,
                config: config_dir(),
                bus: terminal_env(&bus.endpoint, &caller),
            })
    }
}
