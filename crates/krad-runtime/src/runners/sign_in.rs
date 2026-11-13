use futures::channel::mpsc;
use krad_domain::CloudRunnerId;
use krad_host::{quote, Forward, HostError, HostLink};
use krad_protocol::AgentRef;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::io::ErrorKind;

mod callback;

use callback::loopback_callbacks;
pub(crate) use callback::LoopbackCallback;

use crate::terminals::OutputTap;

const STAY_IN_SHELL: &str = "; exec \"${SHELL:-/bin/sh}\" -l";
const ARG_SEPARATOR: &str = " ";
const TAIL_KEPT: usize = 4096;
const TAP_CAPACITY: usize = 64;
const PORT_TAKEN_NOTE: &str = "is already used on this computer, so the browser cannot reach the sign-in on the runner. Free the port and open";
const FORWARD_FAILED: &str = "Could not forward the sign-in page of the runner";

pub(crate) struct SignInWatch {
    pub(crate) runner: CloudRunnerId,
    pub(crate) link: HostLink,
    tail: String,
    seen: HashSet<u16>,
    pub(crate) forwards: Vec<Forward>,
    output: Option<mpsc::Receiver<Vec<u8>>>,
}

impl fmt::Debug for SignInWatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SignInWatch")
            .field("runner", &self.runner)
            .field("forwards", &self.forwards.len())
            .finish_non_exhaustive()
    }
}

impl SignInWatch {
    pub(crate) fn new(runner: CloudRunnerId, link: HostLink) -> Self {
        Self {
            runner,
            link,
            tail: String::new(),
            seen: HashSet::new(),
            forwards: Vec::new(),
            output: None,
        }
    }

    pub(crate) fn tap(&mut self) -> OutputTap {
        let (tap, output) = mpsc::channel(TAP_CAPACITY);
        self.output = Some(output);
        tap
    }

    fn heard_since(&mut self) -> Vec<LoopbackCallback> {
        let mut chunks = Vec::new();
        if let Some(output) = &mut self.output {
            while let Ok(bytes) = output.try_recv() {
                chunks.push(bytes);
            }
        }
        chunks.iter().flat_map(|bytes| self.heard(bytes)).collect()
    }

    fn heard(&mut self, bytes: &[u8]) -> Vec<LoopbackCallback> {
        self.tail.push_str(&String::from_utf8_lossy(bytes));
        let fresh: Vec<LoopbackCallback> = loopback_callbacks(&self.tail)
            .into_iter()
            .filter(|callback| self.seen.insert(callback.port))
            .collect();
        if self.tail.len() > TAIL_KEPT {
            let mut cut = self.tail.len() - TAIL_KEPT;
            while !self.tail.is_char_boundary(cut) {
                cut += 1;
            }
            self.tail.drain(..cut);
        }
        fresh
    }
}

#[derive(Debug, Default)]
pub(crate) struct SignIns(HashMap<AgentRef, SignInWatch>);

impl SignIns {
    pub(crate) fn watch(&mut self, terminal: AgentRef, watch: SignInWatch) {
        self.0.insert(terminal, watch);
    }

    pub(crate) fn heard(&mut self) -> Vec<(AgentRef, HostLink, LoopbackCallback)> {
        self.0
            .iter_mut()
            .flat_map(|(terminal, watch)| {
                let link = watch.link.clone();
                watch
                    .heard_since()
                    .into_iter()
                    .map(move |callback| (terminal.clone(), link.clone(), callback))
            })
            .collect()
    }

    pub(crate) fn get_mut(&mut self, terminal: &AgentRef) -> Option<&mut SignInWatch> {
        self.0.get_mut(terminal)
    }

    pub(crate) fn finished(&mut self, runs: impl Fn(&AgentRef) -> bool) -> Vec<SignInWatch> {
        let ended: Vec<AgentRef> = self
            .0
            .keys()
            .filter(|terminal| !runs(terminal))
            .cloned()
            .collect();
        ended
            .iter()
            .filter_map(|terminal| self.0.remove(terminal))
            .collect()
    }
}

pub(crate) fn login_script(program: &str, args: &[&str]) -> String {
    std::iter::once(quote(program))
        .chain(args.iter().map(|arg| quote(arg)))
        .collect::<Vec<_>>()
        .join(ARG_SEPARATOR)
        + STAY_IN_SHELL
}

pub(crate) fn forward_failure(callback: &LoopbackCallback, error: &HostError) -> String {
    match error {
        HostError::Io(io) if io.kind() == ErrorKind::AddrInUse => {
            format!("Port {} {PORT_TAKEN_NOTE} {}", callback.port, callback.url)
        }
        other => format!("{FORWARD_FAILED} ({}): {other}", callback.url),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_login_runs_and_leaves_a_shell_behind() {
        assert_eq!(
            login_script("codex", &["login", "--device-auth"]),
            "'codex' 'login' '--device-auth'; exec \"${SHELL:-/bin/sh}\" -l"
        );
    }

    #[test]
    fn a_callback_split_across_reads_is_found_once() {
        let mut watch = SignInWatch::new(CloudRunnerId(1), HostLink::Local);
        assert_eq!(watch.heard(b"open http://localhost:14").len(), 0);
        let heard = watch.heard(b"55/auth/callback\r\n");
        assert_eq!(heard.len(), 1);
        assert_eq!(heard[0].port, 1455);
        assert_eq!(watch.heard(b"again http://localhost:1455/x\n").len(), 0);
        watch.heard(&[b'x'; TAIL_KEPT * 2]);
        assert!(watch.tail.len() <= TAIL_KEPT);
    }

    #[test]
    fn tapped_output_is_heard_and_an_ended_terminal_finishes_its_watch(
    ) -> Result<(), mpsc::TrySendError<Vec<u8>>> {
        let mut watch = SignInWatch::new(CloudRunnerId(1), HostLink::Local);
        let mut tap = watch.tap();
        tap.try_send(b"Listening on http://localhost:1455/cb\n".to_vec())?;
        let terminal = AgentRef {
            environment: krad_domain::EnvironmentName::new("api".to_owned()),
            terminal: krad_domain::TerminalId(4),
        };
        let mut sign_ins = SignIns::default();
        sign_ins.watch(terminal.clone(), watch);
        let heard = sign_ins.heard();
        assert_eq!(heard.len(), 1);
        assert_eq!(heard[0].2.port, 1455);
        assert!(sign_ins.finished(|_| true).is_empty());
        assert_eq!(sign_ins.finished(|_| false).len(), 1);
        assert!(sign_ins.get_mut(&terminal).is_none());
        Ok(())
    }

    #[test]
    fn a_busy_port_shows_the_url_to_open() {
        let callback = LoopbackCallback {
            port: 1455,
            url: "http://localhost:1455/auth/callback".to_owned(),
        };
        let busy = HostError::Io(std::io::Error::from(ErrorKind::AddrInUse));
        let text = forward_failure(&callback, &busy);
        assert!(text.starts_with("Port 1455 is already used"));
        assert!(text.ends_with("http://localhost:1455/auth/callback"));
    }
}
