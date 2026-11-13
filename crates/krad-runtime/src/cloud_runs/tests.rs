use super::*;
use krad_domain::{EnvironmentName, TerminalId};
use std::path::Path;
use std::sync::mpsc::sync_channel;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn site() -> Result<CloudSite, Box<dyn std::error::Error>> {
    Ok(CloudSite {
        link: HostLink::Local,
        host: Host::Local,
        cli: Path::new("/nonexistent/krad-ade-agents").to_path_buf(),
        pairing: Pairing {
            install: "0123456789abcdef".parse()?,
            token: "00112233445566778899aabbccddeeff".parse()?,
        },
        placement: CloudPlacement {
            runner: CloudRunnerId(5),
            repo_dir: PathBuf::from("/r/repos/api"),
            worktree: PathBuf::from("/r/worktrees/api/api-a1"),
            branch: "krad/api-a1".to_owned(),
        },
        claude_token: None,
    })
}

fn node() -> AgentRef {
    AgentRef {
        environment: EnvironmentName::new("main".to_owned()),
        terminal: TerminalId(7),
    }
}

fn saved(offset: u64) -> Result<CloudProcess, Box<dyn std::error::Error>> {
    Ok(CloudProcess {
        id: "api-a1-7-f00dbe".parse()?,
        offset: JournalOffset(offset),
    })
}

#[test]
fn a_node_without_a_process_starts_a_fresh_one_and_keeps_it() -> TestResult {
    let (inbox, _input) = sync_channel(4);
    let mut runs = CloudRuns::default();
    let started = runs.start(&node(), &site()?, None, &Jobs::new(inbox));
    let Ok(start) = started else {
        return Err("a fresh process did not start".into());
    };
    assert!(start.fresh);
    assert_eq!(start.from, JournalOffset::default());
    assert!(start.id.as_str().starts_with("api-a1-7-"));
    assert_eq!(
        runs.take_writes(),
        [ProcessWrite::Started(
            node(),
            CloudProcess {
                id: start.id,
                offset: JournalOffset::default()
            }
        )]
    );
    Ok(())
}

#[test]
fn a_kept_process_is_checked_once_then_attached_from_its_offset_or_replaced() -> TestResult {
    let (inbox, _input) = sync_channel(4);
    let jobs = Jobs::new(inbox);
    let mut runs = CloudRuns::default();
    let site = site()?;
    assert_eq!(
        runs.start(&node(), &site, Some(saved(40)?), &jobs),
        Err(StartWait::Waiting)
    );
    assert_eq!(
        runs.start(&node(), &site, Some(saved(40)?), &jobs),
        Err(StartWait::Waiting)
    );
    assert!(runs.checked(&LivenessChecked {
        node: node(),
        liveness: Liveness::Running,
    }));
    let attached = runs.start(&node(), &site, Some(saved(40)?), &jobs);
    assert_eq!(
        attached,
        Ok(ProcessStart {
            id: saved(40)?.id,
            from: JournalOffset(40),
            fresh: false,
        })
    );
    assert_eq!(
        runs.start(&node(), &site, Some(saved(40)?), &jobs),
        Err(StartWait::Waiting)
    );
    runs.checked(&LivenessChecked {
        node: node(),
        liveness: Liveness::Gone,
    });
    let replaced = runs.start(&node(), &site, Some(saved(40)?), &jobs);
    assert!(replaced.is_ok_and(|start| start.fresh));
    Ok(())
}

#[test]
fn a_trimmed_journal_attaches_again_from_its_start_only_once() -> TestResult {
    let (inbox, _input) = sync_channel(4);
    let jobs = Jobs::new(inbox);
    let mut runs = CloudRuns::default();
    let site = site()?;
    let start = ProcessStart {
        id: saved(40)?.id,
        from: JournalOffset(40),
        fresh: false,
    };
    runs.attach(node(), site.run("claude".to_owned(), &start));
    assert!(runs.rewind(&node()));
    assert_eq!(
        runs.take_writes(),
        [ProcessWrite::Offset(
            node(),
            start.id.clone(),
            JournalOffset::default()
        )]
    );
    let again = runs.start(&node(), &site, Some(saved(40)?), &jobs);
    assert_eq!(again.map(|start| start.from), Ok(JournalOffset::default()));
    runs.attach(node(), site.run("claude".to_owned(), &start));
    assert!(!runs.rewind(&node()));
    Ok(())
}

#[test]
fn a_forgotten_run_keeps_what_it_consumed() -> TestResult {
    let mut runs = CloudRuns::default();
    let start = ProcessStart {
        id: saved(12)?.id,
        from: JournalOffset(12),
        fresh: false,
    };
    runs.attach(node(), site()?.run("codex".to_owned(), &start));
    assert_eq!(runs.consumed().count(), 1);
    runs.forget(&node());
    assert_eq!(
        runs.take_writes(),
        [ProcessWrite::Offset(node(), start.id, JournalOffset(12))]
    );
    assert_eq!(runs.consumed().count(), 0);
    Ok(())
}
