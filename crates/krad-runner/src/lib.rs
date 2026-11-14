#[cfg(unix)]
mod daemon;
#[cfg(all(unix, feature = "runtime"))]
mod hosted;
#[cfg(unix)]
mod journal;
#[cfg(unix)]
mod listing;
#[cfg(unix)]
mod pairings;
#[cfg(unix)]
mod piped;
#[cfg(unix)]
mod process;
#[cfg(unix)]
mod pty;
#[cfg(unix)]
mod restart;
#[cfg(all(unix, not(feature = "runtime")))]
#[path = "serving/processes_only.rs"]
mod serving;
#[cfg(all(unix, feature = "runtime"))]
#[path = "serving/with_runtime.rs"]
mod serving;
#[cfg(all(test, unix))]
mod tests;
#[cfg(unix)]
mod verbs;

#[cfg(unix)]
pub use daemon::RunnerDaemon;
#[cfg(all(unix, feature = "runtime"))]
pub use hosted::{
    bind_runtime_socket, Listener, RunnerProfile, RuntimeServer, Stream, RUNNER_ENVIRONMENT,
};
#[cfg(unix)]
pub use verbs::{pair_over_ssh, run_runner_verb};

#[cfg(not(unix))]
#[must_use]
pub fn run_runner_verb(_verb: &str, _rest: &[String]) -> Option<i32> {
    None
}
