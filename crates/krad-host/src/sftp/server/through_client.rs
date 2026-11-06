use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use super::super::{join, Saved, Sftp, SftpError};
use super::serve;
use super::tests::scratch;
use crate::FileKind;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

fn connect() -> Result<Sftp, Box<dyn std::error::Error>> {
    let (client, host) = UnixStream::pair()?;
    let reader = host.try_clone()?;
    thread::spawn(move || serve(reader, host));
    Ok(Sftp::handshake(
        client.try_clone()?,
        client,
        REQUEST_TIMEOUT,
    )?)
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn mode(path: &str) -> std::io::Result<u32> {
    Ok(fs::metadata(path)?.permissions().mode() & 0o777)
}

#[test]
fn files_are_written_read_listed_renamed_and_removed() -> TestResult {
    let sftp = connect()?;
    let dir = text(&scratch("client-files")?);
    let nested = join(&dir, "src");
    sftp.mkdir(&nested)?;
    let file = join(&nested, "main.rs");
    let body = "fn main() {}\n".repeat(10_000);
    sftp.write_with_mode(&file, body.as_bytes(), Some(0o640))?;
    assert_eq!(mode(&file)?, 0o640);

    assert_eq!(sftp.read(&file, u64::MAX)?, body.as_bytes());
    assert_eq!(sftp.read_range(&file, 3, 6)?, b"main()");
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

    assert!(sftp.remove_dir(&nested).is_err());
    sftp.remove_file(&moved)?;
    sftp.remove_dir(&nested)?;
    assert_eq!(sftp.read_dir(&dir)?.len(), 0);
    fs::remove_dir_all(&dir)?;
    assert!(matches!(sftp.stat(&dir), Err(SftpError::NotFound(_))));
    assert!(matches!(
        sftp.read(&file, u64::MAX),
        Err(SftpError::NotFound(_))
    ));
    Ok(())
}

#[test]
fn private_files_and_folders_are_created_with_their_mode() -> TestResult {
    let sftp = connect()?;
    let dir = text(&scratch("client-modes")?);
    let bus = join(&dir, "bus");
    sftp.mkdir_with_mode(&bus, Some(0o700))?;
    let token = join(&bus, "env.token");
    sftp.write_with_mode(&token, b"secret", Some(0o600))?;
    let cli = join(&dir, "cli");
    sftp.write(&cli, b"#!/bin/sh\n")?;
    sftp.set_permissions(&cli, 0o755)?;
    assert_eq!(mode(&bus)?, 0o700);
    assert_eq!(mode(&token)?, 0o600);
    assert_eq!(mode(&cli)?, 0o755);
    assert_eq!(
        sftp.realpath(".")?,
        text(&std::env::current_dir()?.canonicalize()?)
    );
    fs::remove_dir_all(&dir)?;
    Ok(())
}

#[test]
fn exclusive_creation_links_and_saves_behave_as_with_openssh() -> TestResult {
    let sftp = connect()?;
    let dir_path = scratch("client-links")?;
    let dir = text(&dir_path);
    let file = join(&dir, "new.txt");
    sftp.create_new(&file)?;
    assert!(sftp.create_new(&file).is_err());
    assert_eq!(sftp.stat(&file)?.len, 0);

    let script = dir_path.join("run.sh");
    fs::write(&script, "old")?;
    fs::set_permissions(&script, fs::Permissions::from_mode(0o750))?;
    let alias = join(&dir, "alias.sh");
    sftp.symlink(&text(&script), &alias)?;
    assert_eq!(fs::read_link(&alias)?, script);
    assert_eq!(sftp.lstat(&alias)?.kind, FileKind::Symlink);
    let listed = sftp.read_dir(&dir)?;
    let entry = listed
        .iter()
        .find(|entry| entry.name == "alias.sh")
        .ok_or("link not listed")?;
    assert!(entry.symlink);
    assert_eq!(entry.metadata.kind, FileKind::File);

    assert_eq!(sftp.save(&alias, b"new body")?, Saved::Replaced);
    assert_eq!(fs::read(&script)?, b"new body");
    assert_eq!(mode(&text(&script))?, 0o750);
    assert!(fs::symlink_metadata(&alias)?.file_type().is_symlink());

    sftp.replace_from(&alias, &mut &b"dropped"[..], None)?;
    assert_eq!(fs::read(&script)?, b"new body");
    assert_eq!(sftp.lstat(&alias)?.kind, FileKind::File);
    let leftovers = fs::read_dir(&dir_path)?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains(".krad-save-"))
        .count();
    assert_eq!(leftovers, 0);
    fs::remove_dir_all(&dir_path)?;
    Ok(())
}

#[test]
fn large_files_and_many_callers_share_one_channel() -> TestResult {
    let sftp = Arc::new(connect()?);
    let dir = text(&scratch("client-window")?);
    let file = join(&dir, "big.bin");
    let body: Vec<u8> = (0..3 * 1024 * 1024 + 17)
        .map(|at: u32| u8::try_from(at % 251).unwrap_or_default())
        .collect();
    sftp.write(&file, &body)?;
    assert_eq!(sftp.read(&file, u64::MAX)?, body);
    assert_eq!(
        sftp.read_range(&file, 1_000_000, 70_000)?,
        body[1_000_000..1_070_000]
    );
    let workers: Vec<_> = (0..8)
        .map(|worker| {
            let sftp = Arc::clone(&sftp);
            let dir = dir.clone();
            thread::spawn(move || -> Result<(), SftpError> {
                for round in 0..10 {
                    let file = join(&dir, &format!("{worker}-{round}.txt"));
                    let body = format!("worker {worker} round {round}");
                    sftp.write(&file, body.as_bytes())?;
                    assert_eq!(sftp.read(&file, u64::MAX)?, body.as_bytes());
                }
                Ok(())
            })
        })
        .collect();
    for worker in workers {
        worker.join().map_err(|_| "a worker panicked")??;
    }
    assert_eq!(sftp.read_dir(&dir)?.len(), 81);
    fs::remove_dir_all(&dir)?;
    Ok(())
}
