use std::fs;
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};

use super::MuxError;
use crate::layout::{APP_DIR, APP_VERSION, BIN_DIR, HOST_CLI_PROGRAM};
use crate::platform::HostPlatform;

const GREETING: &str = "KRAD-MUX";
const READY: &str = "ready";
const MISSING: &str = "missing";
const MAX_NOISE_LINES: usize = 200;
const MAX_LINE_BYTES: u64 = 4096;
const NEWLINE: u8 = b'\n';
const SPACE: char = ' ';
const NOTHING_TO_UPLOAD: u64 = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[repr(i32)]
pub enum BootstrapFailure {
    #[error("the host could not store this app's CLI (full disk, quota or permissions)")]
    UploadFailed = 94,
    #[error("the host received a malformed CLI size")]
    BadLength = 95,
    #[error("the host got no answer about the CLI")]
    NoAnswer = 96,
}

impl BootstrapFailure {
    const ALL: [Self; 3] = [Self::UploadFailed, Self::BadLength, Self::NoAnswer];

    #[must_use]
    pub fn from_exit_code(code: i32) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|failure| *failure as i32 == code)
    }

    const fn code(self) -> i32 {
        self as i32
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Greeting {
    pub(crate) platform: HostPlatform,
    pub(crate) cli_ready: bool,
}

pub(crate) fn script() -> String {
    let upload_failed = BootstrapFailure::UploadFailed.code();
    let bad_length = BootstrapFailure::BadLength.code();
    let no_answer = BootstrapFailure::NoAnswer.code();
    format!(
        r#"v='{APP_VERSION}'
d="$HOME/{APP_DIR}/$v/{BIN_DIR}"
f="$d/{HOST_CLI_PROGRAM}"
if [ -x "$f" ] && [ "$("$f" --version 2>/dev/null)" = "$v" ]; then s={READY}; else s={MISSING}; fi
printf '%s %s %s\n' '{GREETING}' "$s" "$(uname -sm)"
IFS= read -r n || exit {no_answer}
case "$n" in ''|*[!0-9]*) exit {bad_length} ;; esac
if [ "$n" -gt 0 ]; then
  t="$f.upload.$$"
  mkdir -p "$d" && head -c "$n" > "$t" && [ "$(wc -c < "$t" | tr -d ' ')" = "$n" ] && chmod 755 "$t" && mv -f "$t" "$f" || {{ rm -f "$t"; exit {upload_failed}; }}
fi
exec "$f" serve"#
    )
}

pub(crate) fn read_greeting(reader: &mut impl BufRead) -> Result<Greeting, MuxError> {
    for _ in 0..MAX_NOISE_LINES {
        let mut line = Vec::new();
        let read = reader.take(MAX_LINE_BYTES).read_until(NEWLINE, &mut line)?;
        if read == 0 {
            return Err(MuxError::Closed);
        }
        let line = String::from_utf8_lossy(&line);
        let Some(rest) = line.trim_end().strip_prefix(GREETING) else {
            continue;
        };
        let Some((state, uname)) = rest.trim_start().split_once(SPACE) else {
            return Err(MuxError::Protocol);
        };
        return Ok(Greeting {
            platform: uname
                .parse()
                .map_err(|_| MuxError::Unsupported(uname.to_owned()))?,
            cli_ready: state == READY,
        });
    }
    Err(MuxError::Protocol)
}

pub(crate) fn answer(
    output: &mut impl Write,
    greeting: &Greeting,
    host_clis: &[(HostPlatform, PathBuf)],
) -> Result<(), MuxError> {
    if greeting.cli_ready {
        output.write_all(format!("{NOTHING_TO_UPLOAD}\n").as_bytes())?;
        return Ok(output.flush()?);
    }
    let cli = host_clis
        .iter()
        .find(|(platform, _)| *platform == greeting.platform)
        .map(|(_, path)| path.as_path())
        .ok_or(MuxError::NoHostCli(greeting.platform))?;
    upload(output, cli)
}

fn upload(output: &mut impl Write, cli: &Path) -> Result<(), MuxError> {
    let size = fs::metadata(cli)?.len();
    output.write_all(format!("{size}\n").as_bytes())?;
    let copied = std::io::copy(&mut fs::File::open(cli)?.take(size), output)?;
    if copied != size {
        return Err(MuxError::Protocol);
    }
    tracing::info!(
        bytes = size,
        "uploaded the host CLI through the sign-in session"
    );
    Ok(output.flush()?)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::BufReader;
    use std::process::{Command, Stdio};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn noise_before_the_greeting_is_skipped() -> TestResult {
        let text = "motd line\nKRAD-MUX missing Linux x86_64\n";
        let greeting = read_greeting(&mut text.as_bytes())?;
        assert_eq!(greeting.platform, HostPlatform::LinuxX86_64);
        assert!(!greeting.cli_ready);
        let mac = "KRAD-MUX ready Darwin arm64\n";
        assert!(matches!(
            read_greeting(&mut mac.as_bytes()),
            Err(MuxError::Unsupported(_))
        ));
        Ok(())
    }

    #[test]
    fn the_script_uploads_the_cli_then_runs_it_under_dash() -> TestResult {
        for shell in ["/bin/sh", "/bin/dash"] {
            if !Path::new(shell).exists() {
                continue;
            }
            let home = std::env::temp_dir().join(format!(
                "krad-mux-boot-{}-{}",
                std::process::id(),
                shell.len()
            ));
            fs::create_dir_all(&home)?;
            let cli = home.join("fake-cli");
            fs::write(
                &cli,
                format!("#!/bin/sh\n[ \"$1\" = --version ] && echo {APP_VERSION} && exit 0\necho served \"$1\"\n"),
            )?;
            let mut child = Command::new(shell)
                .args(["-c", &script()])
                .env("HOME", &home)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()?;
            let mut stdout = BufReader::new(child.stdout.take().ok_or("no stdout")?);
            let greeting_line = {
                let mut line = String::new();
                stdout.read_line(&mut line)?;
                line
            };
            assert!(greeting_line.starts_with("KRAD-MUX missing "), "{shell}");
            let mut stdin = child.stdin.take().ok_or("no stdin")?;
            upload(&mut stdin, &cli)?;
            let mut rest = String::new();
            stdout.read_to_string(&mut rest)?;
            assert_eq!(rest, "served serve\n", "{shell}");
            assert!(child.wait()?.success());
            let installed = home
                .join(APP_DIR)
                .join(APP_VERSION)
                .join(BIN_DIR)
                .join(HOST_CLI_PROGRAM);
            assert!(installed.is_file());
            fs::remove_dir_all(&home)?;
        }
        Ok(())
    }
}
