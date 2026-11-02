use std::fs::{self, File};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::error::ExtensionError;
use crate::platform::{mark_executable, Os};

const TAR: &str = "tar";
const GZIP: &str = "gzip";
const UNZIP: &str = "unzip";
const TAR_SUFFIXES: [&str; 5] = [".tar.gz", ".tgz", ".tar.xz", ".tar.bz2", ".tar"];
const ZIP_SUFFIXES: [&str; 3] = [".zip", ".sit", ".vsix"];
const GZ_SUFFIX: &str = ".gz";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Tar,
    Zip,
    Gz,
    Plain,
}

fn kind_of(name: &str) -> Kind {
    let lower = name.to_ascii_lowercase();
    if TAR_SUFFIXES.iter().any(|suffix| lower.ends_with(suffix)) {
        Kind::Tar
    } else if ZIP_SUFFIXES.iter().any(|suffix| lower.ends_with(suffix)) {
        Kind::Zip
    } else if lower.ends_with(GZ_SUFFIX) {
        Kind::Gz
    } else {
        Kind::Plain
    }
}

fn run(command: &mut Command, tool: &str) -> Result<(), ExtensionError> {
    let status = command.stdin(Stdio::null()).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(ExtensionError::Tool(tool.to_owned()))
    }
}

pub(crate) fn place(
    downloaded: &Path,
    name: &str,
    into: &Path,
    os: Os,
) -> Result<(), ExtensionError> {
    fs::create_dir_all(into)?;
    match kind_of(name) {
        Kind::Zip if os == Os::Linux => run(
            Command::new(UNZIP)
                .arg("-qo")
                .arg(downloaded)
                .arg("-d")
                .arg(into),
            UNZIP,
        )?,
        Kind::Tar | Kind::Zip => run(
            Command::new(TAR)
                .arg("-xf")
                .arg(downloaded)
                .arg("-C")
                .arg(into),
            TAR,
        )?,
        Kind::Gz => {
            let target = into.join(&name[..name.len() - GZ_SUFFIX.len()]);
            let out = File::create(&target)?;
            run(
                Command::new(GZIP).arg("-dc").arg(downloaded).stdout(out),
                GZIP,
            )?;
            mark_executable(&target)?;
        }
        Kind::Plain => {
            let target = into.join(name);
            fs::copy(downloaded, &target)?;
            mark_executable(&target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_decides_how_a_download_is_placed() {
        assert_eq!(kind_of("jdtls.tar.gz"), Kind::Tar);
        assert_eq!(kind_of("kotlin-server.sit"), Kind::Zip);
        assert_eq!(kind_of("rust-analyzer-aarch64-apple-darwin.gz"), Kind::Gz);
        assert_eq!(kind_of("rustup-init"), Kind::Plain);
    }
}
