mod agents;
mod api_info;
mod exit;
mod memory;
mod render;
mod request;
mod status;
mod variants;
mod verb;
mod watch;

pub(crate) use exit::exit_codes_help;
pub(crate) use request::{DEFAULT_READ_LINES, DEFAULT_WAIT_TIMEOUT};

use crate::client::call;
use crate::discovery::ApiFile;
use crate::env::{bus_env, guide, orchestrator_guide, CLI_PROGRAM, ENV_TERMINAL};
use crate::extension_tools::ExtensionToolOutcome;
use crate::handle::Caller;
use crate::hook::{run_ask, run_hook};
use crate::protocol::{Request, Response};
use crate::statusline::run_status_line;
use agents::api_action;
use exit::Exit;
use krad_domain::AgentPreset;
use render::render;
use request::{build_request, parse_options, Options};
use status::AgentStates;
use std::ffi::OsStr;
use std::path::Path;
use verb::{usage, ApiAction, Verb};

pub const CLI_NAME: &str = "krad";
const LEAD_TOPIC: &str = "lead";
const FLAG_VERSION: &str = "--version";
const OUTSIDE_TERMINAL: &str = "Run this inside a Krad ADE terminal, or start Krad ADE with \"Let other programs find the agents API\" on in Settings.";

enum Output {
    Stdout(String),
    Stderr(String),
}

#[must_use]
pub fn run_process(api_file: Option<&ApiFile>) -> i32 {
    let mut words = std::env::args_os();
    let invoked = words.next().unwrap_or_default();
    let args: Vec<String> = words
        .map(|word| word.to_string_lossy().into_owned())
        .collect();
    run_invoked(&invoked, &args, api_file)
}

#[must_use]
pub fn run_invoked(invoked: &OsStr, args: &[String], api_file: Option<&ApiFile>) -> i32 {
    if krad_host::mux::is_askpass_invocation(invoked) {
        return crate::askpass::run(args.first().map_or("", String::as_str));
    }
    run_as(program_name(invoked), args, api_file)
}

#[must_use]
pub fn run_cli(args: &[String]) -> i32 {
    run_as(CLI_PROGRAM, args, None)
}

fn program_name(invoked: &OsStr) -> &'static str {
    let stem = Path::new(invoked).file_stem();
    if stem == Some(OsStr::new(CLI_NAME)) {
        CLI_NAME
    } else {
        CLI_PROGRAM
    }
}

fn run_as(program: &str, args: &[String], api_file: Option<&ApiFile>) -> i32 {
    if let Some(code) = crate::host_verbs::run(args) {
        return code;
    }
    if let Some(code) = crate::askpass::run_verb(args) {
        return code;
    }
    if let Some(code) = crate::toolchains::run(args) {
        return code;
    }
    let Some((verb, rest)) = args.split_first() else {
        emit(Output::Stdout(guide(program)));
        return Exit::Ok.code();
    };
    if verb == FLAG_VERSION {
        emit(Output::Stdout(krad_host::HOST_CLI_BUILD.to_owned()));
        return Exit::Ok.code();
    }
    let verb = match verb.parse::<Verb>() {
        Ok(verb) => verb,
        Err(error) => {
            emit(Output::Stderr(format!("{error}\n{}", usage(program))));
            return Exit::Usage.code();
        }
    };
    if matches!(verb, Verb::Hook | Verb::Ask | Verb::StatusLine) {
        let Some((harness, args)) = harness_args(rest) else {
            return Exit::Ok.code();
        };
        return match verb {
            Verb::Ask => run_ask(harness),
            Verb::StatusLine => run_status_line(harness, args),
            _ => run_hook(harness, args),
        };
    }
    if verb == Verb::Mcp {
        return crate::mcp::run_mcp();
    }
    if verb == Verb::Help && rest.first().map(String::as_str) == Some(LEAD_TOPIC) {
        emit(Output::Stdout(orchestrator_guide(program)));
        return Exit::Ok.code();
    }
    let parsed =
        parse_options(rest).and_then(|options| Ok((build_request(verb, &options)?, options)));
    match parsed {
        Ok((Some(request), options)) => execute(verb, &request, &options, api_file),
        Ok((None, _)) => {
            emit(Output::Stdout(guide(program)));
            Exit::Ok.code()
        }
        Err(error) => {
            emit(Output::Stderr(format!("{error}\n{}", usage(program))));
            Exit::Usage.code()
        }
    }
}

fn harness_args(rest: &[String]) -> Option<(AgentPreset, &[String])> {
    let (harness, args) = rest.split_first()?;
    Some((harness.parse().ok()?, args))
}

fn execute(verb: Verb, request: &Request, options: &Options, api_file: Option<&ApiFile>) -> i32 {
    let json = options.json;
    let Some(bus) = bus_env(api_file) else {
        emit(Output::Stderr(OUTSIDE_TERMINAL.to_owned()));
        return Exit::Bus.code();
    };
    let from = match bus.terminal.parse::<Caller>() {
        Ok(from) => from,
        Err(error) => {
            emit(Output::Stderr(format!("{ENV_TERMINAL}: {error}")));
            return Exit::Bus.code();
        }
    };
    if let Request::SubscribeAgents(subscription) = request {
        return watch::run(&bus, &from, subscription.clone(), json);
    }
    if verb == Verb::Api && api_action(options) == Ok(ApiAction::Info) {
        return api_info::run(&bus, &from, options.token, json);
    }
    match call(&bus.endpoint, &bus.token, &from, request.clone()) {
        Ok(response) => {
            let (output, exit) = report(verb, request, &response, json);
            emit(output);
            exit.code()
        }
        Err(error) => {
            emit(Output::Stderr(error.to_string()));
            Exit::Bus.code()
        }
    }
}

fn exit_of(response: &Response) -> Exit {
    match response {
        Response::Error(error) => error.code.into(),
        Response::TimedOut => Exit::TimedOut,
        Response::ExtensionTool(answer) if answer.outcome == ExtensionToolOutcome::Failed => {
            Exit::Failed
        }
        _ => Exit::Ok,
    }
}

fn report(verb: Verb, request: &Request, response: &Response, json: bool) -> (Output, Exit) {
    let exit = exit_of(response);
    let output = if json {
        let line = match response {
            Response::Agents(agents) if verb == Verb::Status => {
                serde_json::to_string(&AgentStates::sort(agents))
            }
            _ => serde_json::to_string(response),
        };
        match line {
            Ok(line) => Output::Stdout(line),
            Err(error) => Output::Stderr(error.to_string()),
        }
    } else if let Response::Error(error) = response {
        Output::Stderr(error.message.clone())
    } else {
        Output::Stdout(render(verb, request, response))
    };
    (output, exit)
}

#[allow(clippy::print_stdout, clippy::print_stderr)]
fn emit(output: Output) {
    match output {
        Output::Stdout(text) => println!("{text}"),
        Output::Stderr(text) => eprintln!("{text}"),
    }
}

#[cfg(test)]
mod tests;
