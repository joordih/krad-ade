use futures::channel::oneshot;
use krad_agents::{
    terminal_env, Caller, ErrorCode, PromptAnswer, Request, Response, ASKPASS_WAIT, CLI_PROGRAM,
};
use krad_domain::PromptAsker;
use krad_host::mux::{askpass_alias, PromptSender, SignInPrompt};
use krad_repo::Askpass;
use std::path::PathBuf;
use std::sync::mpsc;

use crate::bus::BusAccess;

const GIT_ONLY: &str = "Only git asks for a sign-in through the bus";
const ANSWER_THREAD: &str = "krad-runtime-git-askpass";

#[derive(Debug)]
pub(crate) enum AskpassPlan {
    Off,
    Local {
        cli_dir: PathBuf,
        config: Option<PathBuf>,
        bus: Vec<(String, String)>,
    },
    Ready(Askpass),
}

impl AskpassPlan {
    pub(crate) fn local(access: &BusAccess, caller: &Caller) -> Self {
        access
            .cli_dir
            .clone()
            .map_or(Self::Off, |cli_dir| Self::Local {
                cli_dir,
                config: access.config_dir.clone(),
                bus: terminal_env(&access.endpoint, caller),
            })
    }

    pub(crate) fn resolve(self) -> Askpass {
        match self {
            Self::Off => Askpass::default(),
            Self::Ready(askpass) => askpass,
            Self::Local {
                cli_dir,
                config,
                bus,
            } => {
                let cli = cli_dir.join(format!("{CLI_PROGRAM}{}", std::env::consts::EXE_SUFFIX));
                if !cli.is_file() {
                    tracing::debug!("git runs without an askpass program: no CLI beside the app");
                    return Askpass::default();
                }
                askpass_alias(&cli, config.as_deref()).map_or_else(
                    |error| {
                        tracing::debug!(%error, "git runs without an askpass program");
                        Askpass::default()
                    },
                    |program| Askpass::new(program, bus),
                )
            }
        }
    }
}

fn reply_with(reply: oneshot::Sender<Response>, response: Response) {
    if reply.send(response).is_err() {
        tracing::trace!("git left before its sign-in answer");
    }
}

#[tracing::instrument(skip_all)]
pub(crate) fn answer_git(
    request: Request,
    reply: oneshot::Sender<Response>,
    prompts: Option<PromptSender>,
) {
    let Request::Askpass { prompt } = request else {
        reply_with(reply, Response::error(ErrorCode::Forbidden, GIT_ONLY));
        return;
    };
    let (answer, answered) = mpsc::channel();
    let sent = prompts.is_some_and(|prompts| {
        prompts
            .send(SignInPrompt::new(PromptAsker::Git, prompt, answer))
            .is_ok()
    });
    if !sent {
        reply_with(reply, Response::Answered(None));
        return;
    }
    let waiting = std::thread::Builder::new()
        .name(ANSWER_THREAD.to_owned())
        .spawn(move || {
            let answer = answered.recv_timeout(ASKPASS_WAIT).ok().flatten();
            reply_with(
                reply,
                Response::Answered(
                    answer.map(|secret| PromptAnswer::new(secret.expose().to_owned())),
                ),
            );
        });
    if let Err(error) = waiting {
        tracing::warn!(%error, "could not wait for a git sign-in answer");
    }
}
