use super::*;
use std::time::{Duration, UNIX_EPOCH};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const OTHER_PID: u32 = 1;
const STARTED: Duration = Duration::from_secs(1_700_000_000);

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("krad-api-file-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).unwrap_or_default();
    dir
}

fn discovery(pid: u32) -> ApiDiscovery {
    ApiDiscovery {
        api_version: API_VERSION,
        endpoint: SocketAddr::from(([127, 0, 0, 1], 40_123)),
        token: BusToken::from("the-token"),
        pid,
        started_at: Timestamp::from(UNIX_EPOCH + STARTED),
    }
}

#[test]
fn the_file_names_the_version_endpoint_token_pid_and_start() -> TestResult {
    let dir = folder("content");
    let file = ApiFile::in_config(&dir);
    assert_eq!(file.path, dir.join("api.json"));
    file.write(&discovery(std::process::id()))?;
    let written: serde_json::Value = serde_json::from_slice(&std::fs::read(&file.path)?)?;
    assert_eq!(written["api_version"], 1);
    assert_eq!(written["endpoint"], "127.0.0.1:40123");
    assert_eq!(written["token"], "the-token");
    assert_eq!(written["pid"], std::process::id());
    assert_eq!(written["started_at"], "2023-11-14T22:13:20.000Z");
    assert_eq!(file.read()?, Some(discovery(std::process::id())));
    assert!(!format!("{:?}", discovery(OTHER_PID)).contains("the-token"));
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn a_rewrite_is_atomic_and_only_the_owner_reads_it() -> TestResult {
    let dir = folder("atomic");
    let file = ApiFile::in_config(&dir);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(file.tmp_path(), b"half written by a crash")?;
    file.write(&discovery(OTHER_PID))?;
    file.write(&discovery(std::process::id()))?;
    assert_eq!(file.read()?, Some(discovery(std::process::id())));
    let names: Vec<_> = std::fs::read_dir(&dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .collect();
    assert_eq!(names, [std::ffi::OsString::from(API_FILE_NAME)]);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&file.path)?.permissions().mode();
        assert_eq!(mode & 0o777, FileMode::PRIVATE_FILE.0);
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn a_clean_exit_removes_only_its_own_file() -> TestResult {
    let dir = folder("removal");
    let file = ApiFile::in_config(&dir);
    file.remove_if_written_by(std::process::id())?;
    file.write(&discovery(OTHER_PID))?;
    file.remove_if_written_by(std::process::id())?;
    assert!(file.path.exists());
    file.write(&discovery(std::process::id()))?;
    file.remove_if_written_by(std::process::id())?;
    assert!(!file.path.exists());
    assert_eq!(file.read()?, None);
    file.write(&discovery(OTHER_PID))?;
    file.remove()?;
    file.remove()?;
    assert!(!file.path.exists());
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn a_file_is_stale_when_nobody_answers_or_another_process_does() {
    let answer = |pid| ApiInfo {
        pid,
        ..ApiInfo::current()
    };
    assert_eq!(liveness(OTHER_PID, None), Liveness::NotAnswering);
    assert_eq!(
        liveness(OTHER_PID, Some(&answer(Some(OTHER_PID + 1)))),
        Liveness::OtherProcess
    );
    assert_eq!(
        liveness(OTHER_PID, Some(&answer(Some(OTHER_PID)))),
        Liveness::Live
    );
    assert_eq!(liveness(OTHER_PID, Some(&answer(None))), Liveness::Live);
}
