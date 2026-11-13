#![cfg(unix)]

#[path = "support/cloud.rs"]
mod cloud;
#[path = "support/repo.rs"]
mod repo;

use cloud::{
    cloned_with_remote, create_cloud_session, environment, environment_data, install_id, news,
    send, start, temp_folder, use_fake_runner, Started, TestResult, RUNNER,
};
use krad_domain::{RunnerCli, RunnerLogin};
use krad_host::{APP_DIR, HOST_CLI_BUILD, HOST_CLI_PROGRAM};
use krad_protocol::{CloudCommand, CloudNews, Command, Reply, RunnerCommand};
use repo::{committed_repo, git};
use std::error::Error;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;

const PAIRING: &str = "00112233445566778899aabbccddeeff";
const BRANCH_PREFIX: &str = "krad/";
const EXECUTABLE: u32 = 0o755;
const BIN_DIR: &str = "bin";
const PAIR_VERB: &str = "cloud-pair";

fn install_fake_cli(runner_home: &Path) -> Result<(), Box<dyn Error>> {
    let bin = runner_home.join(APP_DIR).join(HOST_CLI_BUILD).join(BIN_DIR);
    std::fs::create_dir_all(&bin)?;
    let cli = bin.join(HOST_CLI_PROGRAM);
    let script = format!(
        "#!/bin/sh\ncase \"$1\" in\n  --version) echo '{HOST_CLI_BUILD}' ;;\n  {PAIR_VERB}) echo '{PAIRING}' ;;\n  *) exit 0 ;;\nesac\n"
    );
    std::fs::write(&cli, script)?;
    std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(EXECUTABLE))?;
    Ok(())
}

#[test]
fn a_cloud_session_is_prepared_on_a_ready_runner_and_its_branch_comes_home() -> TestResult {
    let config = temp_folder("cs-config")?;
    let runner_home = temp_folder("cs-home")?;
    use_fake_runner(&config.0, &runner_home.0);
    let origin = committed_repo("cs-origin")?;
    git(
        &origin.0,
        &["config", "receive.denyCurrentBranch", "ignore"],
    )?;
    let local = cloned_with_remote(&origin, "cs-local")?;
    let root = local.0.join("work");
    install_fake_cli(&runner_home.0)?;
    let started = start(&root, install_id()?)?;
    let (logins, opened) = create_cloud_session(&started)?;
    let Started {
        runtime,
        group,
        feed,
    } = started;
    let environment = environment();
    assert_eq!(logins.runner, RUNNER);
    let login_of = |cli: RunnerCli| {
        logins
            .logins
            .iter()
            .find(|known| known.cli == cli)
            .map(|known| known.login)
    };
    assert_eq!(login_of(RunnerCli::Claude), Some(RunnerLogin::SignedOut));
    assert_eq!(login_of(RunnerCli::Github), Some(RunnerLogin::SignedIn));
    assert_eq!(login_of(RunnerCli::Gitlab), Some(RunnerLogin::SignedOut));
    assert_eq!(opened.group, group);
    assert_eq!(opened.starts.len(), 1);
    let data = environment_data(&runtime)?;
    let placement = data
        .session(opened.session)
        .and_then(|session| session.cloud.clone())
        .ok_or("the opened session is not placed on the runner")?;
    assert_eq!(placement.runner, RUNNER);
    assert!(placement.branch.starts_with(BRANCH_PREFIX));
    assert!(placement.worktree.starts_with(&runner_home.0));
    assert!(placement.worktree.join(repo::TRACKED).is_file());

    let push = CloudCommand::Push {
        environment: environment.clone(),
        session: opened.session,
    };
    assert_eq!(send(&runtime, Command::Cloud(push))?, Reply::Accepted);
    let pushed = news(&feed, |news| matches!(news, CloudNews::Pushed { .. }))?;
    assert_eq!(
        pushed,
        CloudNews::Pushed {
            branch: placement.branch.clone()
        }
    );
    git(
        &origin.0,
        &["rev-parse", "--verify", "--quiet", &placement.branch],
    )?;

    let home = CloudCommand::BringHome {
        environment,
        session: opened.session,
    };
    assert_eq!(send(&runtime, Command::Cloud(home))?, Reply::Accepted);
    let brought = news(&feed, |news| {
        matches!(news, CloudNews::BroughtHome { .. } | CloudNews::NoRepo)
    })?;
    assert_eq!(
        brought,
        CloudNews::BroughtHome {
            branch: placement.branch.clone()
        }
    );
    assert_eq!(environment_data(&runtime)?.groups.len(), 2);

    assert_eq!(
        send(
            &runtime,
            Command::Runner(RunnerCommand::Remove { runner: RUNNER })
        )?,
        Reply::Accepted
    );
    Ok(())
}
