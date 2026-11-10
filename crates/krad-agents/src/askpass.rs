use std::io::{self, Write};

use crate::client::call;
use crate::env::bus_env;
use crate::handle::Caller;
use crate::protocol::{Request, Response};

pub const ASKPASS_VERB: &str = "askpass";
const EXIT_OK: i32 = 0;
const EXIT_NO_ANSWER: i32 = 1;
const EXIT_FAILED: i32 = 125;
const NEWLINE: &[u8] = b"\n";

pub(crate) fn run_verb(args: &[String]) -> Option<i32> {
    let (verb, rest) = args.split_first()?;
    (verb == ASKPASS_VERB).then(|| run(rest.first().map_or("", String::as_str)))
}

pub(crate) fn run(prompt: &str) -> i32 {
    if std::env::var_os(krad_host::mux::ENV_ENDPOINT).is_some() {
        return krad_host::mux::run_askpass(prompt);
    }
    let Some(bus) = bus_env(None) else {
        return EXIT_NO_ANSWER;
    };
    let Ok(from) = bus.terminal.parse::<Caller>() else {
        return EXIT_FAILED;
    };
    let request = Request::Askpass {
        prompt: prompt.to_owned(),
    };
    match call(&bus.endpoint, &bus.token, &from, request) {
        Ok(Response::Answered(Some(answer))) => {
            let mut out = io::stdout();
            let written = out
                .write_all(answer.expose().as_bytes())
                .and_then(|()| out.write_all(NEWLINE))
                .and_then(|()| out.flush());
            if written.is_ok() {
                EXIT_OK
            } else {
                EXIT_FAILED
            }
        }
        Ok(_) => EXIT_NO_ANSWER,
        Err(_) => EXIT_FAILED,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_askpass_verb_answers_a_prompt() {
        assert_eq!(run_verb(&["list".to_owned()]), None);
        assert_eq!(run_verb(&[]), None);
    }
}
