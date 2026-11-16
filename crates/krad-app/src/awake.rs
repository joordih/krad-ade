use krad_domain::{AgentState, EnvironmentName, TerminalId};
use krad_protocol::{Scope, Snapshot};
use krad_runtime::RuntimeHandle;
use std::collections::HashMap;
use std::io;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const AWAKE_CHECK: Duration = Duration::from_secs(20);
const WAITING_KEPT_AWAKE: Duration = Duration::from_hours(2);
const AWAKE_THREAD: &str = "krad-ade-service-awake";
#[cfg(target_os = "macos")]
const HOLD: [&str; 2] = ["caffeinate", "-i"];
#[cfg(not(target_os = "macos"))]
const HOLD: [&str; 7] = [
    "systemd-inhibit",
    "--what=idle:sleep",
    "--who=Krad ADE",
    "--why=agents are working",
    "--mode=block",
    "sleep",
    "infinity",
];

type Node = (EnvironmentName, TerminalId);

#[derive(Default)]
struct Awake {
    waiting_since: HashMap<Node, Instant>,
    hold: Option<Child>,
}

impl Awake {
    fn wanted(&mut self, snapshot: &Snapshot, now: Instant) -> bool {
        let mut working = false;
        let mut waiting = HashMap::new();
        for environment in &snapshot.environments {
            for node in &environment.nodes {
                match node.state {
                    AgentState::Working => working = true,
                    AgentState::WaitingForInput => {
                        let key = (environment.name.clone(), node.id);
                        let since = self.waiting_since.get(&key).copied().unwrap_or(now);
                        waiting.insert(key, since);
                    }
                    AgentState::Idle | AgentState::Done => {}
                }
            }
        }
        let recent_wait = waiting
            .values()
            .any(|since| now.duration_since(*since) < WAITING_KEPT_AWAKE);
        self.waiting_since = waiting;
        working || recent_wait
    }

    fn holding(&mut self) -> bool {
        self.hold
            .as_mut()
            .is_some_and(|hold| matches!(hold.try_wait(), Ok(None)))
    }

    fn keep(&mut self, wanted: bool) {
        if !wanted {
            return self.release();
        }
        if self.holding() {
            return;
        }
        let [program, args @ ..] = HOLD;
        self.hold = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .inspect_err(|error| tracing::warn!(%error, "this computer could not be kept awake"))
            .ok();
        if self.hold.is_some() {
            tracing::info!("keeping this computer awake for its agents");
        }
    }

    fn release(&mut self) {
        let Some(mut hold) = self.hold.take() else {
            return;
        };
        if let Err(error) = hold.kill().and_then(|()| hold.wait().map(drop)) {
            tracing::debug!(%error, "the awake hold had already ended");
        }
        tracing::info!("this computer may sleep again");
    }
}

/// # Errors
///
/// Fails when the watching thread cannot start.
pub(crate) fn keep_awake_for_agents(runtime: RuntimeHandle) -> io::Result<()> {
    std::thread::Builder::new()
        .name(AWAKE_THREAD.to_owned())
        .spawn(move || {
            let mut awake = Awake::default();
            loop {
                let Ok(snapshot) = futures::executor::block_on(runtime.snapshot(Scope::Runtime))
                else {
                    awake.release();
                    return;
                };
                let wanted = awake.wanted(&snapshot, Instant::now());
                awake.keep(wanted);
                std::thread::sleep(AWAKE_CHECK);
            }
        })
        .map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot_with(state: AgentState) -> Result<Snapshot, serde_json::Error> {
        let mut data = krad_domain::EnvironmentData::empty(EnvironmentName::new("main".to_owned()));
        data.nodes.push(serde_json::from_value(serde_json::json!({
            "id": 1, "title": "agent", "state": state, "group": null, "kind": "Terminal"
        }))?);
        let mut snapshot: Snapshot =
            serde_json::from_value(serde_json::json!({ "scope": "runtime", "seq": 1 }))?;
        snapshot.environments = vec![data];
        Ok(snapshot)
    }

    #[test]
    fn a_working_agent_keeps_it_awake_and_a_waiting_one_only_for_two_hours(
    ) -> Result<(), serde_json::Error> {
        let mut awake = Awake::default();
        let start = Instant::now();
        assert!(awake.wanted(&snapshot_with(AgentState::Working)?, start));
        let waiting = snapshot_with(AgentState::WaitingForInput)?;
        assert!(awake.wanted(&waiting, start));
        assert!(awake.wanted(&waiting, start + WAITING_KEPT_AWAKE / 2));
        assert!(!awake.wanted(&waiting, start + WAITING_KEPT_AWAKE));
        assert!(!awake.wanted(&snapshot_with(AgentState::Idle)?, start));
        assert!(awake.wanted(&waiting, start + WAITING_KEPT_AWAKE * 3));
        Ok(())
    }
}
