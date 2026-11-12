use super::*;
use krad_domain::Host;
use krad_host::runner::{INSTALL_ENV, TOKEN_ENV};
use krad_host::HostContext;
use std::ffi::OsString;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const TOKEN: &str = "00112233445566778899aabbccddeeff";
const OAUTH: &str = "sk-ant-oat-secret";

fn run(from: u64) -> Result<CloudRun, Box<dyn std::error::Error>> {
    Ok(CloudRun::new(
        HostLink::new(
            &Host::Ssh {
                destination: "runner".parse()?,
            },
            &HostContext::default(),
        ),
        PathBuf::from("/home/me/.krad-ade/bin/krad-ade-agents"),
        Pairing {
            install: "0123456789abcdef".parse()?,
            token: TOKEN.parse()?,
        },
        "claude".to_owned(),
        "chat-1".parse()?,
        JournalOffset(from),
    ))
}

fn env() -> Vec<(String, String)> {
    vec![
        ("CLAUDE_CODE_OAUTH_TOKEN".to_owned(), OAUTH.to_owned()),
        ("PATH".to_owned(), "/home/me/.krad-ade/bin".to_owned()),
        ("KRAD_ADE_TOKEN_FILE".to_owned(), "/home/me/t".to_owned()),
    ]
}

#[test]
fn the_attach_names_the_process_its_offset_and_the_program_to_start() -> TestResult {
    let command = run(42)?.attach_command(
        Path::new("/home/me/.krad-ade/runner/worktrees/api/fix"),
        &["-p".to_owned()],
        &env(),
    );
    let args: Vec<String> = command
        .args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        args,
        [
            "cloud-attach",
            "chat-1",
            "42",
            "--cwd",
            "/home/me/.krad-ade/runner/worktrees/api/fix",
            "--pass",
            "CLAUDE_CODE_OAUTH_TOKEN",
            "--pass",
            "KRAD_ADE_TOKEN_FILE",
            "--",
            "claude",
            "-p"
        ]
    );
    assert_eq!(
        command.env,
        [(
            OsString::from(INSTALL_ENV),
            OsString::from("0123456789abcdef")
        )]
    );
    assert_eq!(
        command.env_from_stdin,
        [TOKEN_ENV, "CLAUDE_CODE_OAUTH_TOKEN", "KRAD_ADE_TOKEN_FILE"].map(OsString::from)
    );
    assert_eq!(
        command.path_prefix,
        Some(PathBuf::from("/home/me/.krad-ade/bin"))
    );
    assert!(command.through_login_shell);
    Ok(())
}

#[test]
fn every_secret_reaches_the_runner_on_stdin_never_in_the_script() -> TestResult {
    let built = run(0)?.command(Path::new("/srv/api"), &[], &env())?;
    let argv: String = std::iter::once(built.command.get_program())
        .chain(built.command.get_args())
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert!(!argv.contains(TOKEN));
    assert!(!argv.contains(OAUTH));
    assert_eq!(built.input, Some(format!("{TOKEN}\n{OAUTH}\n/home/me/t\n")));
    Ok(())
}

#[test]
fn a_cloud_chat_runs_like_a_remote_one_and_one_off_commands_skip_the_daemon() -> TestResult {
    let cloud = run(0)?;
    let runner = Runner::Cloud(cloud.clone());
    assert!(runner.on_ssh());
    assert!(runner.in_cloud());
    assert_eq!(runner.link(), cloud.link);
    assert_eq!(runner.served_directory(Path::new("/srv/api")), "/srv/api");
    assert_eq!(
        cloud.host_runner(),
        Runner::Ssh {
            link: cloud.link.clone(),
            program: "claude".to_owned()
        }
    );
    assert_eq!(cloud.consumed(), JournalOffset(0));
    Ok(())
}
