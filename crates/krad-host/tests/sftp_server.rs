use krad_host::sftp::{join, Sftp, SftpError};
use krad_host::FileKind;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

const SFTP_SERVER_VAR: &str = "KRAD_SFTP_SERVER";
const SFTP_SERVER_NAME: &str = "sftp-server";
const SFTP_SERVER_DIRS: [&str; 7] = [
    "/usr/libexec",
    "/usr/libexec/openssh",
    "/usr/lib/openssh",
    "/usr/lib/ssh",
    "/usr/lib",
    "/usr/local/libexec",
    "C:/Windows/System32/OpenSSH",
];

fn sftp_server() -> Option<PathBuf> {
    if let Some(chosen) = std::env::var_os(SFTP_SERVER_VAR) {
        return Some(PathBuf::from(chosen)).filter(|path| path.is_file());
    }
    let file = format!("{SFTP_SERVER_NAME}{}", std::env::consts::EXE_SUFFIX);
    let on_path: Vec<PathBuf> = std::env::var_os("PATH")
        .map_or_else(Vec::new, |path| std::env::split_paths(&path).collect());
    on_path
        .into_iter()
        .chain(SFTP_SERVER_DIRS.iter().map(PathBuf::from))
        .map(|dir| dir.join(&file))
        .find(|path| path.is_file())
}

struct Server {
    child: Child,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.child.kill().ok();
        self.child.wait().ok();
    }
}

fn server() -> Result<Option<(Server, Sftp)>, Box<dyn std::error::Error>> {
    let Some(program) = sftp_server() else {
        return Ok(None);
    };
    let mut child = Command::new(program)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    let stdin = child.stdin.take().ok_or("no stdin")?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let sftp = Sftp::handshake(stdout, stdin, REQUEST_TIMEOUT)?;
    Ok(Some((Server { child }, sftp)))
}

fn scratch(name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(format!("krad-host-{name}-{}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    std::fs::create_dir_all(&dir)?;
    Ok(dir.canonicalize()?)
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[test]
fn files_are_written_read_listed_renamed_and_removed() -> TestResult {
    let Some((_server, sftp)) = server()? else {
        return Ok(());
    };
    let dir = text(&scratch("files")?);
    let nested = join(&dir, "src");
    sftp.mkdir(&nested)?;
    let file = join(&nested, "main.rs");
    let body = "fn main() {}\n".repeat(10_000);
    sftp.write(&file, body.as_bytes())?;

    assert_eq!(sftp.read(&file, u64::MAX)?, body.as_bytes());
    assert_eq!(sftp.read(&file, 4)?, b"fn m");
    assert_eq!(sftp.read_range(&file, 3, 6)?, b"main()");
    let far = u64::try_from(body.len())? - 2;
    assert_eq!(sftp.read_range(&file, far, u64::MAX)?, b"}\n");
    let metadata = sftp.stat(&file)?;
    assert_eq!(metadata.kind, FileKind::File);
    assert_eq!(metadata.len, u64::try_from(body.len())?);
    assert!(metadata.modified.is_some());

    let entries = sftp.read_dir(&dir)?;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "src");
    assert_eq!(entries[0].metadata.kind, FileKind::Directory);

    let moved = join(&nested, "lib.rs");
    sftp.write(&moved, b"old")?;
    sftp.rename(&file, &moved)?;
    assert_eq!(sftp.read(&moved, u64::MAX)?, body.as_bytes());
    assert!(sftp.stat(&file).is_err_and(|error| error.is_not_found()));

    std::fs::remove_dir_all(&dir)?;
    assert!(matches!(sftp.stat(&dir), Err(SftpError::NotFound(_))));
    Ok(())
}

#[test]
fn realpath_resolves_the_starting_folder() -> TestResult {
    let Some((_server, sftp)) = server()? else {
        return Ok(());
    };
    let home = sftp.realpath(".")?;
    assert!(home.starts_with('/'));
    Ok(())
}

#[cfg(unix)]
#[test]
fn exclusive_creation_refuses_a_taken_name_and_lstat_sees_links() -> TestResult {
    let Some((_server, sftp)) = server()? else {
        return Ok(());
    };
    let dir_path = scratch("links")?;
    let dir = text(&dir_path);
    let file = join(&dir, "new.txt");
    sftp.create_new(&file)?;
    assert!(sftp.create_new(&file).is_err());
    assert_eq!(sftp.stat(&file)?.len, 0);

    let folder = join(&dir, "folder");
    sftp.mkdir(&folder)?;
    std::os::unix::fs::symlink(&folder, dir_path.join("link"))?;
    let link = join(&dir, "link");
    assert_eq!(sftp.lstat(&link)?.kind, FileKind::Symlink);
    assert_eq!(sftp.stat(&link)?.kind, FileKind::Directory);
    let listed = sftp.read_dir(&dir)?;
    let entry = listed
        .iter()
        .find(|entry| entry.name == "link")
        .ok_or("link not listed")?;
    assert!(entry.symlink);
    assert_eq!(entry.metadata.kind, FileKind::Directory);

    std::fs::remove_dir_all(&dir)?;
    assert!(!dir_path.exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn private_files_and_folders_are_created_with_their_mode() -> TestResult {
    use std::os::unix::fs::PermissionsExt;

    let Some((_server, sftp)) = server()? else {
        return Ok(());
    };
    let dir_path = scratch("modes")?;
    let dir = text(&dir_path);
    let bus = join(&dir, "bus");
    sftp.mkdir_with_mode(&bus, Some(0o700))?;
    let token = join(&bus, "env.token");
    sftp.write_with_mode(&token, b"secret", Some(0o600))?;
    sftp.set_permissions(&token, 0o600)?;
    let cli = join(&dir, "cli");
    sftp.write(&cli, b"#!/bin/sh\n")?;
    sftp.set_permissions(&cli, 0o755)?;
    let mode = |path: &str| -> std::io::Result<u32> {
        Ok(std::fs::metadata(path)?.permissions().mode() & 0o777)
    };
    assert_eq!(mode(&bus)?, 0o700);
    assert_eq!(mode(&token)?, 0o600);
    assert_eq!(mode(&cli)?, 0o755);
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn links_are_created_target_first_and_writes_keep_the_file_in_place() -> TestResult {
    use std::os::unix::fs::PermissionsExt;

    let Some((_server, sftp)) = server()? else {
        return Ok(());
    };
    let dir_path = scratch("in-place")?;
    let dir = text(&dir_path);
    let script = join(&dir, "run.sh");
    sftp.write(&script, b"old")?;
    std::fs::set_permissions(
        dir_path.join("run.sh"),
        std::fs::Permissions::from_mode(0o755),
    )?;
    let link = join(&dir, "alias.sh");
    sftp.symlink(&script, &link)?;
    assert_eq!(
        std::fs::read_link(dir_path.join("alias.sh"))?,
        dir_path.join("run.sh")
    );

    sftp.write(&link, b"new")?;
    assert_eq!(sftp.lstat(&link)?.kind, FileKind::Symlink);
    assert_eq!(std::fs::read(dir_path.join("run.sh"))?, b"new");
    assert_eq!(
        std::fs::metadata(dir_path.join("run.sh"))?
            .permissions()
            .mode()
            & 0o777,
        0o755
    );

    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

#[test]
fn many_callers_share_one_channel_at_once() -> TestResult {
    let Some((_server, sftp)) = server()? else {
        return Ok(());
    };
    let sftp = Arc::new(sftp);
    let dir = text(&scratch("concurrent")?);
    let workers: Vec<_> = (0..8)
        .map(|worker| {
            let sftp = Arc::clone(&sftp);
            let dir = dir.clone();
            std::thread::spawn(move || -> Result<(), SftpError> {
                for round in 0..25 {
                    let file = join(&dir, &format!("{worker}-{round}.txt"));
                    let body = format!("worker {worker} round {round}");
                    sftp.write(&file, body.as_bytes())?;
                    assert_eq!(sftp.read(&file, u64::MAX)?, body.as_bytes());
                    assert_eq!(
                        sftp.stat(&file)?.len,
                        u64::try_from(body.len()).unwrap_or(0)
                    );
                }
                Ok(())
            })
        })
        .collect();
    for worker in workers {
        worker.join().map_err(|_| "a worker panicked")??;
    }
    assert_eq!(sftp.read_dir(&dir)?.len(), 200);
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

#[test]
fn large_files_move_through_a_window_of_requests() -> TestResult {
    let Some((_server, sftp)) = server()? else {
        return Ok(());
    };
    let dir = text(&scratch("window")?);
    let file = join(&dir, "big.bin");
    let body: Vec<u8> = (0..3 * 1024 * 1024 + 17)
        .map(|at: u32| (at % 251) as u8)
        .collect();
    sftp.write(&file, &body)?;
    assert_eq!(sftp.read(&file, u64::MAX)?, body);
    assert_eq!(
        sftp.read_range(&file, 1_000_000, 70_000)?,
        body[1_000_000..1_070_000]
    );
    let streamed = join(&dir, "streamed.bin");
    let sent = sftp.write_from(&streamed, &mut body.as_slice(), Some(0o640))?;
    assert_eq!(sent, u64::try_from(body.len())?);
    assert_eq!(std::fs::read(&streamed)?, body);
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn saving_replaces_the_file_atomically_keeping_mode_and_links() -> TestResult {
    use std::os::unix::fs::PermissionsExt;

    let Some((_server, sftp)) = server()? else {
        return Ok(());
    };
    let dir_path = scratch("save")?;
    let dir = text(&dir_path);
    let script = dir_path.join("run.sh");
    std::fs::write(&script, "old")?;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o750))?;
    std::os::unix::fs::symlink(&script, dir_path.join("alias.sh"))?;

    assert_eq!(
        sftp.save(&join(&dir, "alias.sh"), b"new body")?,
        krad_host::sftp::Saved::Replaced
    );
    assert_eq!(std::fs::read(&script)?, b"new body");
    assert_eq!(
        std::fs::metadata(&script)?.permissions().mode() & 0o777,
        0o750
    );
    assert!(std::fs::symlink_metadata(dir_path.join("alias.sh"))?
        .file_type()
        .is_symlink());
    sftp.save(&join(&dir, "fresh.txt"), b"first")?;
    assert_eq!(std::fs::read(dir_path.join("fresh.txt"))?, b"first");
    let leftovers = std::fs::read_dir(&dir_path)?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains(".krad-save-"))
        .count();
    assert_eq!(leftovers, 0);
    std::fs::remove_dir_all(&dir_path)?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_host_that_stops_answering_times_out_instead_of_hanging() -> TestResult {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;

    let (client, mut server) = UnixStream::pair()?;
    let stalled = std::thread::spawn(move || -> std::io::Result<()> {
        let mut init = [0; 9];
        server.read_exact(&mut init)?;
        server.write_all(&[0, 0, 0, 5, 2, 0, 0, 0, 3])?;
        let mut sink = [0; 1024];
        while server.read(&mut sink)? > 0 {}
        Ok(())
    });
    let sftp = Sftp::handshake(client.try_clone()?, client, Duration::from_millis(300))?;
    let started = std::time::Instant::now();
    assert!(matches!(sftp.stat("/anything"), Err(SftpError::TimedOut)));
    assert!(started.elapsed() < Duration::from_secs(5));
    drop(sftp);
    drop(stalled);
    Ok(())
}

#[cfg(unix)]
#[test]
fn replacing_a_link_over_sftp_never_writes_through_it() -> TestResult {
    let Some((_server, sftp)) = server()? else {
        return Ok(());
    };
    let dir_path = scratch("replace-link")?;
    let dir = text(&dir_path);
    let outside = dir_path.join("outside.txt");
    std::fs::write(&outside, "elsewhere")?;
    let link = join(&dir, "a.txt");
    std::os::unix::fs::symlink(&outside, dir_path.join("a.txt"))?;
    sftp.replace_from(&link, &mut &b"dropped"[..], None)?;
    assert_eq!(std::fs::read_to_string(&outside)?, "elsewhere");
    assert_eq!(sftp.lstat(&link)?.kind, FileKind::File);
    assert_eq!(sftp.read(&link, u64::MAX)?, b"dropped");
    std::fs::remove_dir_all(dir_path)?;
    Ok(())
}
