use krad_host::{remote_path, HostCommand, HostLink};
use std::path::{Path, PathBuf};

use crate::RepoError;

const LIST_SCRIPT: &str = r#"cd "$1" || exit 2
find . -mindepth 1 -name '.*' -prune -o -type f -print | sed 's|^\./||' | head -n "$2""#;
const SEARCH_SCRIPT: &str = r#"cd "$1" || exit 2
if grep -Z -e x /dev/null >/dev/null 2>&1 || [ "$?" -eq 1 ]; then z=-Z; else z=; fi
grep -rnIiF $z --exclude-dir='.?*' -e "$2" . | head -n "$3""#;
const LIST_PROGRAM: &str = "find";
const SEARCH_PROGRAM: &str = "grep";
const NO_MATCH_STATUS: i32 = 1;
const CURRENT_DIR_PREFIX: &str = "./";
const NAME_END: char = '\0';
const FIELD_SEPARATOR: char = ':';

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HostMatch {
    pub(crate) path: PathBuf,
    pub(crate) line: u32,
    pub(crate) text: String,
}

fn script(body: &str, root: &Path, rest: &[String]) -> HostCommand {
    let mut command = HostCommand::posix_shell(body);
    command.arg(remote_path(root)).args(rest);
    command
}

fn run(host: &HostLink, command: &HostCommand, program: &str) -> Result<String, RepoError> {
    let output = host.output(command)?;
    if output.status.success() || output.status.code() == Some(NO_MATCH_STATUS) {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    Err(RepoError::Git {
        command: program.to_owned(),
        message: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    })
}

pub(crate) fn list_files(
    host: &HostLink,
    root: &Path,
    limit: usize,
) -> Result<Vec<PathBuf>, RepoError> {
    let listed = run(
        host,
        &script(LIST_SCRIPT, root, &[limit.to_string()]),
        LIST_PROGRAM,
    )?;
    Ok(parse_listing(&listed))
}

fn parse_listing(listed: &str) -> Vec<PathBuf> {
    listed
        .lines()
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect()
}

pub(crate) fn search(
    host: &HostLink,
    root: &Path,
    query: &str,
    limit: usize,
) -> Result<Vec<HostMatch>, RepoError> {
    let found = run(
        host,
        &script(SEARCH_SCRIPT, root, &[query.to_owned(), limit.to_string()]),
        SEARCH_PROGRAM,
    )?;
    Ok(parse_matches(&found))
}

fn parse_matches(found: &str) -> Vec<HostMatch> {
    found.lines().filter_map(parse_match).collect()
}

fn parse_match(line: &str) -> Option<HostMatch> {
    let (path, rest) = match line.split_once(NAME_END) {
        Some((path, rest)) => (path, rest),
        None => line.split_once(FIELD_SEPARATOR)?,
    };
    let (number, text) = rest.split_once(FIELD_SEPARATOR)?;
    Some(HostMatch {
        path: PathBuf::from(path.strip_prefix(CURRENT_DIR_PREFIX).unwrap_or(path)),
        line: number.parse().ok()?,
        text: text.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listings_drop_blank_lines() {
        assert_eq!(
            parse_listing("src/main.rs\n\nREADME.md\n"),
            [PathBuf::from("src/main.rs"), PathBuf::from("README.md")]
        );
    }

    #[test]
    fn matches_read_nul_separated_names_with_colons() {
        assert_eq!(
            parse_match("./a:b.txt\u{0}12:let x = 1;"),
            Some(HostMatch {
                path: PathBuf::from("a:b.txt"),
                line: 12,
                text: "let x = 1;".to_owned(),
            })
        );
    }

    #[test]
    fn matches_fall_back_to_colon_fields_without_nul() {
        assert_eq!(
            parse_match("./src/lib.rs:3:pub mod x;"),
            Some(HostMatch {
                path: PathBuf::from("src/lib.rs"),
                line: 3,
                text: "pub mod x;".to_owned(),
            })
        );
        assert_eq!(parse_match("garbage"), None);
    }

    #[test]
    fn the_scripts_take_their_values_as_arguments() {
        let command = script(SEARCH_SCRIPT, Path::new("/srv/app"), &["it's".to_owned()]);
        assert_eq!(
            command
                .args
                .last()
                .map(|arg| arg.to_string_lossy().into_owned()),
            Some("it's".to_owned())
        );
    }
}
