use krad_domain::{AgentPreset, HarnessHealth, HarnessVersion, SignIn};
use krad_host::{local_output_within, HostLink};
use serde::de::IgnoredAny;
use serde::Deserialize;
use std::path::Path;
use std::time::Duration;

use crate::process::{harness_command, Runner};

const PROBE_TIMEOUT: Duration = Duration::from_secs(15);
const VERSION_ARGS: &[&str] = &["--version"];
const CLAUDE_STATUS_ARGS: &[&str] = &["auth", "status"];
const CODEX_STATUS_ARGS: &[&str] = &["login", "status"];
const OPENCODE_JSON_ARGS: &[&str] = &["auth", "list", "--format", "json"];
const OPENCODE_TEXT_ARGS: &[&str] = &["auth", "list"];
const OPENCODE_JSON_SINCE: HarnessVersion = HarnessVersion::new(2, 0, 0);
const CODEX_SIGNED_OUT: &str = "not logged in";
const CODEX_SIGNED_IN: &str = "logged in";
const OPENCODE_CREDENTIAL_WORDS: [&str; 2] = ["credentials", "credential"];
const OPENCODE_STORED_WORD: &str = "stored";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProbeOutput {
    stdout: String,
    stderr: String,
}

impl ProbeOutput {
    fn text(&self) -> String {
        format!("{}\n{}", self.stdout, self.stderr)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SignInProbe {
    ClaudeStatus,
    CodexStatus,
    OpencodeJson,
    OpencodeText,
}

impl SignInProbe {
    fn of(harness: AgentPreset, version: HarnessVersion) -> Option<Self> {
        let minimum = harness.setup()?.minimum;
        match harness {
            AgentPreset::ClaudeCode => (version >= minimum).then_some(Self::ClaudeStatus),
            AgentPreset::Codex => Some(Self::CodexStatus),
            AgentPreset::Opencode if version >= OPENCODE_JSON_SINCE => Some(Self::OpencodeJson),
            AgentPreset::Opencode => Some(Self::OpencodeText),
            AgentPreset::Shell | AgentPreset::Gateway | AgentPreset::Acp(_) => None,
        }
    }

    const fn args(self) -> &'static [&'static str] {
        match self {
            Self::ClaudeStatus => CLAUDE_STATUS_ARGS,
            Self::CodexStatus => CODEX_STATUS_ARGS,
            Self::OpencodeJson => OPENCODE_JSON_ARGS,
            Self::OpencodeText => OPENCODE_TEXT_ARGS,
        }
    }

    fn read(self, output: &ProbeOutput) -> SignIn {
        match self {
            Self::ClaudeStatus => claude_sign_in(&output.stdout),
            Self::CodexStatus => codex_sign_in(&output.text()),
            Self::OpencodeJson => opencode_json_sign_in(&output.stdout),
            Self::OpencodeText => opencode_text_sign_in(&output.text()),
        }
    }
}

#[derive(Deserialize)]
struct ClaudeAuthStatus {
    #[serde(rename = "loggedIn")]
    logged_in: bool,
}

#[derive(Deserialize)]
struct OpencodeIntegration {
    #[serde(default)]
    connections: Vec<IgnoredAny>,
}

const fn signed_in_if(signed_in: bool) -> SignIn {
    if signed_in {
        SignIn::SignedIn
    } else {
        SignIn::SignedOut
    }
}

fn claude_sign_in(stdout: &str) -> SignIn {
    serde_json::from_str::<ClaudeAuthStatus>(stdout.trim())
        .map_or(SignIn::Unknown, |status| signed_in_if(status.logged_in))
}

fn codex_sign_in(text: &str) -> SignIn {
    let said = text.to_lowercase();
    if said.contains(CODEX_SIGNED_OUT) {
        return SignIn::SignedOut;
    }
    if said.contains(CODEX_SIGNED_IN) {
        return SignIn::SignedIn;
    }
    SignIn::Unknown
}

fn opencode_json_sign_in(stdout: &str) -> SignIn {
    serde_json::from_str::<Vec<OpencodeIntegration>>(stdout.trim()).map_or(
        SignIn::Unknown,
        |integrations| {
            signed_in_if(
                integrations
                    .iter()
                    .any(|integration| !integration.connections.is_empty()),
            )
        },
    )
}

fn opencode_text_sign_in(text: &str) -> SignIn {
    let counted = text.lines().find_map(|line| {
        let mut words = line.split_whitespace().rev();
        let last = words.next()?;
        OPENCODE_CREDENTIAL_WORDS
            .contains(&last)
            .then(|| words.next()?.parse::<u32>().ok())
            .flatten()
    });
    if let Some(count) = counted {
        return signed_in_if(count > 0);
    }
    let stored = text
        .lines()
        .any(|line| line.split_whitespace().last() == Some(OPENCODE_STORED_WORD));
    if stored {
        SignIn::SignedIn
    } else {
        SignIn::Unknown
    }
}

fn read_health(
    harness: AgentPreset,
    version_output: &ProbeOutput,
    sign_in: impl FnOnce(SignInProbe) -> Option<ProbeOutput>,
) -> HarnessHealth {
    let version = HarnessVersion::find_in(&version_output.text());
    let sign_in = version
        .and_then(|version| SignInProbe::of(harness, version))
        .and_then(|probe| sign_in(probe).map(|output| probe.read(&output)))
        .unwrap_or(SignIn::Unknown);
    HarnessHealth::Found { version, sign_in }
}

fn probe(runner: &Runner, cwd: &Path, args: &[&str]) -> Option<ProbeOutput> {
    let args: Vec<String> = args.iter().map(|&arg| arg.to_owned()).collect();
    let command = harness_command(runner, cwd, &args, &[])
        .inspect_err(|error| tracing::debug!(%error, "could not build a harness check"))
        .ok()?;
    let output = local_output_within(command, PROBE_TIMEOUT)
        .inspect_err(|error| tracing::debug!(%error, "a harness check did not finish"))
        .ok()?;
    Some(ProbeOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

#[must_use]
#[tracing::instrument(skip_all, fields(harness = harness.display_name()))]
pub fn harness_health(runner: &Runner, cwd: &Path, harness: AgentPreset) -> HarnessHealth {
    let Some(version_output) = probe(runner, cwd, VERSION_ARGS) else {
        return HarnessHealth::Found {
            version: None,
            sign_in: SignIn::Unknown,
        };
    };
    read_health(harness, &version_output, |sign_in| {
        probe(runner, cwd, sign_in.args())
    })
}

#[must_use]
pub fn harness_off_path(
    link: &HostLink,
    home: Option<&Path>,
    harness: AgentPreset,
    program: &str,
) -> HarnessHealth {
    let found = harness.setup().and_then(|setup| {
        setup
            .off_path_candidates(program, home)
            .into_iter()
            .find(|candidate| link.exists(candidate))
    });
    if let Some(program) = found {
        return HarnessHealth::OffPath { program };
    }
    tracing::debug!(
        harness = harness.display_name(),
        "harness not found on PATH"
    );
    HarnessHealth::Missing
}

#[cfg(test)]
mod tests;
