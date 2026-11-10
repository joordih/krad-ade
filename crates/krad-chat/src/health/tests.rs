use super::*;

const CLAUDE_SIGNED_IN: &str = r#"{
  "loggedIn": true,
  "authMethod": "claude.ai",
  "apiProvider": "firstParty"
}"#;
const CLAUDE_SIGNED_OUT: &str = r#"{"loggedIn": false, "authMethod": "none"}"#;
const OPENCODE_JSON: &str = r#"[
  {"id": "anthropic", "name": "Anthropic", "connections": [{"type": "credential", "method": "oauth"}]},
  {"id": "openai", "name": "OpenAI", "connections": []}
]"#;
const OPENCODE_V1_TEXT: &str = "┌  Credentials ~/.local/share/opencode/auth.json\n│\n●  Anthropic oauth\n│\n└  1 credentials\n";
const OPENCODE_V1_EMPTY: &str =
    "┌  Credentials ~/.local/share/opencode/auth.json\n│\n└  0 credentials\n";
const OPENCODE_V2_TEXT: &str = "Claude Agent      Claude Agent                stored\n";

fn printed(stdout: &str) -> ProbeOutput {
    ProbeOutput {
        stdout: stdout.to_owned(),
        stderr: String::new(),
    }
}

fn health_with(
    harness: AgentPreset,
    version: &str,
    sign_in: Option<&str>,
) -> (HarnessHealth, Option<SignInProbe>) {
    let mut asked = None;
    let health = read_health(harness, &printed(version), |probe| {
        asked = Some(probe);
        sign_in.map(printed)
    });
    (health, asked)
}

#[test]
fn claude_code_reports_its_sign_in_as_json() {
    assert_eq!(claude_sign_in(CLAUDE_SIGNED_IN), SignIn::SignedIn);
    assert_eq!(claude_sign_in(CLAUDE_SIGNED_OUT), SignIn::SignedOut);
    assert_eq!(claude_sign_in("Usage: claude [options]"), SignIn::Unknown);
}

#[test]
fn codex_says_whether_it_is_logged_in() {
    assert_eq!(codex_sign_in("Logged in using ChatGPT\n"), SignIn::SignedIn);
    assert_eq!(
        codex_sign_in("Logged in using an API key - sk-***\n"),
        SignIn::SignedIn
    );
    assert_eq!(codex_sign_in("Not logged in\n"), SignIn::SignedOut);
    assert_eq!(
        codex_sign_in("error: unrecognized subcommand"),
        SignIn::Unknown
    );
}

#[test]
fn opencode_counts_its_connections() {
    assert_eq!(opencode_json_sign_in(OPENCODE_JSON), SignIn::SignedIn);
    assert_eq!(opencode_json_sign_in("[]"), SignIn::SignedOut);
    assert_eq!(opencode_json_sign_in("not json"), SignIn::Unknown);
    assert_eq!(opencode_text_sign_in(OPENCODE_V1_TEXT), SignIn::SignedIn);
    assert_eq!(opencode_text_sign_in(OPENCODE_V1_EMPTY), SignIn::SignedOut);
    assert_eq!(opencode_text_sign_in(OPENCODE_V2_TEXT), SignIn::SignedIn);
    assert_eq!(opencode_text_sign_in(""), SignIn::Unknown);
}

#[test]
fn a_current_harness_is_asked_for_its_sign_in() {
    let (health, asked) = health_with(
        AgentPreset::ClaudeCode,
        "2.1.319 (Claude Code)\n",
        Some(CLAUDE_SIGNED_IN),
    );
    assert_eq!(asked, Some(SignInProbe::ClaudeStatus));
    assert_eq!(
        health,
        HarnessHealth::Found {
            version: Some(HarnessVersion::new(2, 1, 319)),
            sign_in: SignIn::SignedIn,
        }
    );
}

#[test]
fn a_claude_code_without_auth_status_is_not_asked() {
    let (health, asked) = health_with(AgentPreset::ClaudeCode, "2.0.14 (Claude Code)", None);
    assert_eq!(asked, None);
    assert_eq!(
        health,
        HarnessHealth::Found {
            version: Some(HarnessVersion::new(2, 0, 14)),
            sign_in: SignIn::Unknown,
        }
    );
}

#[test]
fn opencode_is_asked_for_json_from_version_two() {
    let (_, asked) = health_with(AgentPreset::Opencode, "opencode v2.2.6", Some("[]"));
    assert_eq!(asked, Some(SignInProbe::OpencodeJson));
    let (health, asked) = health_with(AgentPreset::Opencode, "1.4.2", Some(OPENCODE_V1_EMPTY));
    assert_eq!(asked, Some(SignInProbe::OpencodeText));
    assert_eq!(
        health,
        HarnessHealth::Found {
            version: Some(HarnessVersion::new(1, 4, 2)),
            sign_in: SignIn::SignedOut,
        }
    );
}

#[test]
fn an_unreadable_version_skips_the_sign_in_check() {
    let (health, asked) = health_with(AgentPreset::Codex, "segmentation fault", None);
    assert_eq!(asked, None);
    assert_eq!(
        health,
        HarnessHealth::Found {
            version: None,
            sign_in: SignIn::Unknown,
        }
    );
}

#[test]
fn a_harness_in_none_of_the_known_folders_is_missing() {
    let home = std::env::temp_dir().join("krad-health-missing-home");
    assert_eq!(
        harness_off_path(
            &HostLink::Local,
            Some(&home),
            AgentPreset::Codex,
            "krad-no-such-harness"
        ),
        HarnessHealth::Missing
    );
}

#[test]
fn a_harness_installed_off_path_is_found_where_it_lives() -> std::io::Result<()> {
    let home = std::env::temp_dir().join(format!("krad-health-home-{}", std::process::id()));
    let folder = home.join(".opencode/bin");
    std::fs::create_dir_all(&folder)?;
    let program = folder.join("krad-test-opencode");
    std::fs::write(&program, b"")?;
    let health = harness_off_path(
        &HostLink::Local,
        Some(&home),
        AgentPreset::Opencode,
        "krad-test-opencode",
    );
    std::fs::remove_dir_all(&home)?;
    assert_eq!(health, HarnessHealth::OffPath { program });
    Ok(())
}
