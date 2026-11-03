use super::quote;

const WATCHER_SCRIPT: &str = r#"exec 3>&2 2>/dev/null 4<&0
if command -v setsid >/dev/null 2>&1; then setsid sh -c "$1" </dev/null 2>&3 3>&- 4<&- & else sh -c "$1" </dev/null 2>&3 3>&- 4<&- & fi
c=$!
k() { for p in $(cat /proc/"$1"/task/*/children 2>/dev/null || pgrep -P "$1" 2>/dev/null); do k "$p"; done; kill -TERM "$1" 2>/dev/null; }
{ cat <&4 >/dev/null 4<&-; kill -TERM -"$c" 2>/dev/null; k "$c"; } & w=$!
exec 4<&-
wait "$c"; s=$?; k "$w"; exit "$s""#;
const WATCHER_SHELL: &str = "exec sh -c";
const WATCHER_NAME: &str = "sh";

pub(super) fn bound_to_stdin(script: &str) -> String {
    format!(
        "{WATCHER_SHELL} {} {WATCHER_NAME} {}",
        quote(WATCHER_SCRIPT),
        quote(script)
    )
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::Write as _;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn shells() -> impl Iterator<Item = &'static str> {
        ["/bin/sh", "/bin/dash", "/bin/bash"]
            .into_iter()
            .filter(|shell| std::path::Path::new(shell).exists())
    }

    #[test]
    fn the_whole_tree_stops_when_stdin_closes_under_every_posix_shell() -> TestResult {
        for shell in shells() {
            let marker = std::env::temp_dir().join(format!(
                "krad-orphan-{}-{}",
                std::process::id(),
                shell.replace('/', "-")
            ));
            let inner = format!("sh -c 'sleep 30' & sleep 0.5; touch '{}'", marker.display());
            let mut child = Command::new(shell)
                .arg("-c")
                .arg(bound_to_stdin(&inner))
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .spawn()?;
            let mut stdin = child.stdin.take().ok_or("no stdin")?;
            stdin.write_all(b"")?;
            drop(stdin);
            let deadline = Instant::now() + Duration::from_secs(5);
            while child.try_wait()?.is_none() {
                assert!(Instant::now() < deadline, "{shell}: outlived its stdin");
                std::thread::sleep(Duration::from_millis(20));
            }
            std::thread::sleep(Duration::from_millis(800));
            assert!(!marker.exists(), "{shell}: a child kept running");
        }
        Ok(())
    }

    #[test]
    fn a_slow_command_runs_to_the_end_while_stdin_stays_open() -> TestResult {
        for shell in shells() {
            let mut child = Command::new(shell)
                .arg("-c")
                .arg(bound_to_stdin("sleep 0.3; printf late; exit 4"))
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()?;
            let open_stdin = child.stdin.take();
            let output = child.wait_with_output()?;
            drop(open_stdin);
            assert_eq!(output.stdout, b"late", "{shell}");
            assert_eq!(output.status.code(), Some(4), "{shell}");
        }
        Ok(())
    }

    #[test]
    fn output_and_status_survive_the_watcher() -> TestResult {
        for shell in shells() {
            let mut child = Command::new(shell)
                .arg("-c")
                .arg(bound_to_stdin("printf out; printf err >&2; exit 3"))
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;
            let open_stdin = child.stdin.take();
            let output = child.wait_with_output()?;
            drop(open_stdin);
            assert_eq!(output.stdout, b"out", "{shell}");
            assert_eq!(output.stderr, b"err", "{shell}");
            assert_eq!(output.status.code(), Some(3), "{shell}");
        }
        Ok(())
    }
}
