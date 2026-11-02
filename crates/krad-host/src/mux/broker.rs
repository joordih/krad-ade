mod pending;
mod relay;

use krad_domain::SshDestination;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use super::prompt::{PromptSender, Secret, SignInPrompt};
use super::session::{Session, Side};
use super::{MuxError, ENV_ENDPOINT, ENV_TOKEN};
use crate::layout::APP_VERSION;
use crate::random::{random_hex, TOKEN_BYTES};
use crate::ssh_link::lock;
use pending::Pending;

const ACCEPT_THREAD: &str = "krad-ade-mux-broker";
const CONNECTION_THREAD: &str = "krad-ade-mux-broker-connection";
const ACCEPT_POLL: Duration = Duration::from_millis(100);
const HEADER_LIMIT_BYTES: u64 = 256;
const PROMPT_LIMIT_BYTES: usize = 8 * 1024;
const SIGN_IN_WAIT: Duration = Duration::from_mins(5);
const HANDSHAKE_WAIT: Duration = Duration::from_secs(10);
const REQUEST_WAIT: Duration = Duration::from_secs(10);
const EXEC_KIND: &str = "exec";
const ASKPASS_KIND: &str = "askpass";
pub(crate) const ANSWER_OK: &str = "ok";
pub(crate) const ANSWER_NONE: &str = "no";
const SPACE: char = ' ';
const NEWLINE: u8 = b'\n';

pub(crate) struct Broker {
    endpoint: SocketAddr,
    token: String,
    pending: Arc<Mutex<Pending>>,
    remote: Arc<Mutex<Option<Session>>>,
    stop: Arc<AtomicBool>,
}

struct Shared {
    token: String,
    destination: SshDestination,
    prompts: Option<PromptSender>,
    pending: Arc<Mutex<Pending>>,
    remote: Arc<Mutex<Option<Session>>>,
}

impl Broker {
    pub(crate) fn start(
        destination: SshDestination,
        prompts: Option<PromptSender>,
    ) -> io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let endpoint = listener.local_addr()?;
        let token = new_token();
        let pending = Arc::new(Mutex::new(Pending::default()));
        let remote = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Shared {
            token: token.clone(),
            destination,
            prompts,
            pending: Arc::clone(&pending),
            remote: Arc::clone(&remote),
        });
        let stopping = Arc::clone(&stop);
        thread::Builder::new()
            .name(ACCEPT_THREAD.to_owned())
            .spawn(move || accept_loop(&listener, &shared, &stopping))?;
        Ok(Self {
            endpoint,
            token,
            pending,
            remote,
            stop,
        })
    }

    pub(crate) fn attach(&self, session: Session) {
        *lock(&self.remote) = Some(session);
    }

    pub(crate) fn register(&self, script: String) -> String {
        lock(&self.pending).insert(script)
    }

    pub(crate) fn env(&self) -> [(&'static str, String); 2] {
        [
            (ENV_ENDPOINT, self.endpoint.to_string()),
            (ENV_TOKEN, self.token.clone()),
        ]
    }
}

impl Drop for Broker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

fn accept_loop(listener: &TcpListener, shared: &Arc<Shared>, stop: &AtomicBool) {
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _)) => {
                let shared = Arc::clone(shared);
                let spawned = thread::Builder::new()
                    .name(CONNECTION_THREAD.to_owned())
                    .spawn(move || {
                        if let Err(error) = serve_connection(&shared, stream) {
                            tracing::debug!(%error, "a local multiplexer client failed");
                        }
                    });
                if let Err(error) = spawned {
                    tracing::warn!(%error, "could not serve a local multiplexer client");
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => thread::sleep(ACCEPT_POLL),
            Err(error) => {
                tracing::warn!(%error, "the local multiplexer broker stopped");
                return;
            }
        }
    }
}

fn serve_connection(shared: &Shared, stream: TcpStream) -> Result<(), MuxError> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(REQUEST_WAIT))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut header = Vec::new();
    (&mut reader)
        .take(HEADER_LIMIT_BYTES)
        .read_until(NEWLINE, &mut header)?;
    let header = String::from_utf8_lossy(&header);
    let mut words = header.trim_end().split(SPACE);
    let token = words.next().unwrap_or_default();
    if !same_token(token, &shared.token) {
        stream.shutdown(Shutdown::Both).ok();
        return Err(MuxError::Protocol);
    }
    match (words.next(), words.next()) {
        (Some(EXEC_KIND), None) => {
            stream.set_read_timeout(None)?;
            serve_exec(shared, reader, stream)
        }
        (Some(ASKPASS_KIND), Some(len)) => {
            let len = len.parse::<usize>().map_err(|_| MuxError::Protocol)?;
            if len > PROMPT_LIMIT_BYTES {
                return Err(MuxError::Protocol);
            }
            let mut prompt = vec![0; len];
            reader.read_exact(&mut prompt)?;
            let answer = ask(shared, String::from_utf8_lossy(&prompt).into_owned());
            reply(stream, answer.as_ref())
        }
        _ => Err(MuxError::Protocol),
    }
}

fn serve_exec(
    shared: &Shared,
    reader: BufReader<TcpStream>,
    stream: TcpStream,
) -> Result<(), MuxError> {
    let closer = stream.try_clone()?;
    let (session, incoming, _) = Session::start(
        reader,
        stream,
        Side::Host,
        APP_VERSION,
        HANDSHAKE_WAIT,
        Some(Box::new(move || {
            closer.shutdown(Shutdown::Both).ok();
        })),
    )?;
    let remote = lock(&shared.remote).clone().ok_or(MuxError::Closed)?;
    for request in incoming {
        relay::run(&shared.pending, &remote, request);
    }
    session.close();
    Ok(())
}

fn ask(shared: &Shared, text: String) -> Option<Secret> {
    let prompts = shared.prompts.as_ref()?;
    let (answer, answered) = mpsc::channel();
    prompts
        .send(SignInPrompt::new(shared.destination.clone(), text, answer))
        .ok()?;
    answered.recv_timeout(SIGN_IN_WAIT).ok().flatten()
}

fn reply(mut stream: TcpStream, answer: Option<&Secret>) -> Result<(), MuxError> {
    let mut message = Vec::new();
    match answer {
        Some(secret) => {
            message
                .extend_from_slice(format!("{ANSWER_OK} {}\n", secret.expose().len()).as_bytes());
            message.extend_from_slice(secret.expose().as_bytes());
        }
        None => message.extend_from_slice(format!("{ANSWER_NONE}\n").as_bytes()),
    }
    stream.write_all(&message)?;
    stream.flush()?;
    stream.shutdown(Shutdown::Both).ok();
    Ok(())
}

fn same_token(given: &str, expected: &str) -> bool {
    given.len() == expected.len()
        && given
            .bytes()
            .zip(expected.bytes())
            .fold(0, |difference, (left, right)| difference | (left ^ right))
            == 0
}

fn new_token() -> String {
    random_hex(TOKEN_BYTES)
}

pub(crate) fn exec_header(token: &str) -> String {
    format!("{token}{SPACE}{EXEC_KIND}\n")
}

pub(crate) fn askpass_header(token: &str, prompt: &str) -> String {
    format!("{token}{SPACE}{ASKPASS_KIND}{SPACE}{}\n", prompt.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_compare_whole_and_differ_between_brokers() {
        let token = new_token();
        assert!(same_token(&token, &token));
        assert!(!same_token(&token[..10], &token));
        assert_ne!(new_token(), token);
    }

    #[test]
    fn an_askpass_question_reaches_the_prompts_and_its_answer_comes_back(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (prompts, asked) = mpsc::channel();
        let broker = Broker::start("devbox".parse()?, Some(prompts))?;
        let answering = thread::spawn(move || {
            if let Ok(prompt) = asked.recv() {
                let text = prompt.text.clone();
                prompt.answer(Some(Secret::new("s3cret".to_owned())));
                return text;
            }
            String::new()
        });
        let mut stream = TcpStream::connect(broker.endpoint)?;
        let prompt = "me@devbox's password: ";
        stream.write_all(askpass_header(&broker.token, prompt).as_bytes())?;
        stream.write_all(prompt.as_bytes())?;
        let mut answer = String::new();
        stream.read_to_string(&mut answer)?;
        assert_eq!(answer, "ok 6\ns3cret");
        assert_eq!(answering.join().map_err(|_| "panicked")?, prompt);
        Ok(())
    }

    #[test]
    fn a_wrong_token_gets_nothing() -> Result<(), Box<dyn std::error::Error>> {
        let broker = Broker::start("devbox".parse()?, None)?;
        let mut stream = TcpStream::connect(broker.endpoint)?;
        stream.write_all(askpass_header("0000", "x").as_bytes())?;
        stream.write_all(b"x")?;
        let mut answer = String::new();
        stream.read_to_string(&mut answer).ok();
        assert_eq!(answer, "");
        Ok(())
    }
}
