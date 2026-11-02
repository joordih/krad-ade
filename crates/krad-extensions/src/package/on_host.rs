use krad_host::{remote_path, HostCommand, HostLink};
use std::path::Path;

use crate::archive::Kind;
use crate::error::ExtensionError;
use crate::http::Sha256Digest;
use crate::package::output::tail;

const FETCH_SCRIPT: &str = r#"d="$1"; url="$2"; name="$3"; kind="$4"; sum="$5"
mkdir -p "$d" || exit 1
f="$d/$name.download"
if command -v curl >/dev/null 2>&1; then curl -fsSL --retry 2 -o "$f" "$url" || { rm -f "$f"; exit 94; }
elif command -v wget >/dev/null 2>&1; then wget -q -O "$f" "$url" || { rm -f "$f"; exit 94; }
else exit 90; fi
if [ -n "$sum" ]; then
  command -v sha256sum >/dev/null 2>&1 || { rm -f "$f"; exit 91; }
  got=$(sha256sum "$f" | cut -d' ' -f1)
  [ "$got" = "$sum" ] || { rm -f "$f"; exit 92; }
fi
case "$kind" in
  tar) tar -xf "$f" -C "$d" || { rm -f "$f"; exit 1; } ;;
  zip) command -v unzip >/dev/null 2>&1 || { rm -f "$f"; exit 93; }; unzip -qo "$f" -d "$d" || { rm -f "$f"; exit 1; } ;;
  gz) gzip -dc "$f" > "$d/${name%.gz}" && chmod +x "$d/${name%.gz}" || { rm -f "$f"; exit 1; } ;;
  *) cp "$f" "$d/$name" && chmod +x "$d/$name" || { rm -f "$f"; exit 1; } ;;
esac
rm -f "$f""#;
const EXECUTABLES_SCRIPT: &str = r#"find "$1" -type f ! -perm -u+x | while IFS= read -r f; do
  m=$(head -c 4 "$f" 2>/dev/null | od -An -tx1 | tr -d ' \n')
  case "$m" in 7f454c46|2321*) chmod +x "$f" ;; esac
done"#;
const FETCH_PROGRAM: &str = "download on the host";
const EXECUTABLES_PROGRAM: &str = "mark executables";
const NO_DOWNLOADER: i32 = 90;
const NO_CHECKSUM_TOOL: i32 = 91;
const DIGEST_MISMATCH: i32 = 92;
const NO_UNZIP: i32 = 93;
const DOWNLOAD_FAILED: i32 = 94;

#[derive(Debug)]
pub(crate) enum HostFetch {
    Done,
    Unavailable,
}

pub(crate) struct Download<'a> {
    pub(crate) url: &'a str,
    pub(crate) name: &'a str,
    pub(crate) kind: Kind,
    pub(crate) digest: Option<&'a Sha256Digest>,
}

pub(crate) fn fetch(
    link: &HostLink,
    download: &Download<'_>,
    into: &Path,
) -> Result<HostFetch, ExtensionError> {
    let mut command = HostCommand::posix_shell(FETCH_SCRIPT);
    command.args([
        remote_path(into),
        download.url.to_owned(),
        download.name.to_owned(),
        download.kind.label().to_owned(),
        download
            .digest
            .map_or_else(String::new, |digest| digest.hex().to_owned()),
    ]);
    let output = link.output(&command)?;
    match output.status.code() {
        Some(0) => Ok(HostFetch::Done),
        Some(NO_DOWNLOADER | NO_CHECKSUM_TOOL | NO_UNZIP | DOWNLOAD_FAILED) => {
            tracing::info!(code = ?output.status.code(), "the host cannot download the package itself");
            Ok(HostFetch::Unavailable)
        }
        Some(DIGEST_MISMATCH) => Err(ExtensionError::Digest {
            url: download.url.to_owned(),
        }),
        status => Err(ExtensionError::ToolFailed {
            program: FETCH_PROGRAM.to_owned(),
            status,
            output: tail(&output.stderr),
        }),
    }
}

pub(crate) fn mark_executables(link: &HostLink, dir: &Path) -> Result<(), ExtensionError> {
    let mut command = HostCommand::posix_shell(EXECUTABLES_SCRIPT);
    command.arg(remote_path(dir));
    let output = link.output(&command)?;
    crate::package::output::checked(EXECUTABLES_PROGRAM, &output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn scratch(name: &str) -> Result<std::path::PathBuf, std::io::Error> {
        let dir = std::env::temp_dir().join(format!("krad-fetch-{name}-{}", std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    #[cfg(unix)]
    #[test]
    fn executables_are_found_by_their_magic_bytes() -> TestResult {
        use std::os::unix::fs::PermissionsExt;

        let dir = scratch("exec")?;
        std::fs::write(dir.join("server"), b"\x7fELF\x02\x01")?;
        std::fs::write(dir.join("run.sh"), b"#!/bin/sh\necho hi\n")?;
        std::fs::write(dir.join("README"), b"plain text")?;
        let status = Command::new("/bin/sh")
            .args(["-c", EXECUTABLES_SCRIPT, "sh"])
            .arg(&dir)
            .status()?;
        assert!(status.success());
        let mode = |name: &str| -> std::io::Result<u32> {
            Ok(std::fs::metadata(dir.join(name))?.permissions().mode() & 0o100)
        };
        assert_eq!(mode("server")?, 0o100);
        assert_eq!(mode("run.sh")?, 0o100);
        assert_eq!(mode("README")?, 0);
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn a_wrong_checksum_is_refused_and_leaves_nothing_behind() -> TestResult {
        let dir = scratch("digest")?;
        let source = dir.join("source.bin");
        std::fs::write(&source, b"payload")?;
        let into = dir.join("into");
        let url = format!("file://{}", source.display());
        let status = Command::new("/bin/sh")
            .args(["-c", FETCH_SCRIPT, "sh"])
            .arg(&into)
            .args([url.as_str(), "tool", "plain", &"0".repeat(64)])
            .status()?;
        let has_sha = Command::new("/bin/sh")
            .args(["-c", "command -v sha256sum"])
            .status()?
            .success();
        let expected = if has_sha {
            DIGEST_MISMATCH
        } else {
            NO_CHECKSUM_TOOL
        };
        assert_eq!(status.code(), Some(expected));
        assert!(!into.join("tool.download").exists());
        assert!(!into.join("tool").exists());
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }
}
