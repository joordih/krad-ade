use std::fs;
use std::path::{Path, PathBuf};

const SSHD_CONFIG: &str = "/etc/ssh/sshd_config";
const SUBSYSTEM_KEYWORD: &str = "subsystem";
const SFTP_NAME: &str = "sftp";
const COMMENT: char = '#';
const KNOWN_LOCATIONS: [&str; 6] = [
    "/usr/lib/openssh/sftp-server",
    "/usr/libexec/openssh/sftp-server",
    "/usr/lib/ssh/sftp-server",
    "/usr/libexec/sftp-server",
    "/usr/lib/misc/sftp-server",
    "/usr/sbin/sftp-server",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SftpServer {
    pub(super) program: PathBuf,
    pub(super) args: Vec<String>,
}

pub(super) fn locate() -> Option<SftpServer> {
    configured(&fs::read_to_string(SSHD_CONFIG).unwrap_or_default())
        .filter(|server| server.program.is_file())
        .or_else(|| {
            KNOWN_LOCATIONS
                .iter()
                .map(Path::new)
                .find(|path| path.is_file())
                .map(|program| SftpServer {
                    program: program.to_owned(),
                    args: Vec::new(),
                })
        })
}

fn configured(config: &str) -> Option<SftpServer> {
    config.lines().find_map(|line| {
        let line = line.split(COMMENT).next().unwrap_or_default();
        let mut words = line.split_whitespace();
        let keyword = words.next()?;
        if !keyword.eq_ignore_ascii_case(SUBSYSTEM_KEYWORD) || words.next()? != SFTP_NAME {
            return None;
        }
        let program = words.next()?;
        program.starts_with('/').then(|| SftpServer {
            program: PathBuf::from(program),
            args: words.map(str::to_owned).collect(),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_configured_sftp_server_wins_unless_it_is_internal() {
        let config =
            "# Subsystem sftp /nope\nSubsystem\tsftp\t/usr/lib/openssh/sftp-server -l INFO\n";
        assert_eq!(
            configured(config),
            Some(SftpServer {
                program: PathBuf::from("/usr/lib/openssh/sftp-server"),
                args: vec!["-l".to_owned(), "INFO".to_owned()],
            })
        );
        assert_eq!(configured("Subsystem sftp internal-sftp\n"), None);
        assert_eq!(configured(""), None);
    }
}
