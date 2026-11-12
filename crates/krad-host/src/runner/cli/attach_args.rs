use krad_domain::{CloudProcessId, JournalOffset};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::super::wire::CloudLaunch;
use crate::mux::TerminalSize;

const CWD_FLAG: &str = "--cwd";
const PASS_FLAG: &str = "--pass";
const TERMINAL_FLAG: &str = "--terminal";
const END_OF_FLAGS: &str = "--";
const PATH_SEPARATOR: char = '/';
const EXECUTABLE_BITS: u32 = 0o111;
pub(super) const PATH_VAR: &str = "PATH";
const HOME_VAR: &str = "HOME";
const ALWAYS_PASSED: [&str; 2] = [PATH_VAR, HOME_VAR];

#[derive(Debug, PartialEq, Eq)]
pub(super) struct AttachArgs {
    pub(super) id: CloudProcessId,
    pub(super) from: JournalOffset,
    pub(super) launch: Option<CloudLaunch>,
}

pub(super) fn parse_attach(
    rest: &[String],
    lookup: impl Fn(&str) -> Option<String>,
) -> Option<AttachArgs> {
    let (id, rest) = rest.split_first()?;
    let (from, mut rest) = rest.split_first()?;
    let id = id.parse().ok()?;
    let from = JournalOffset(from.parse().ok()?);
    let mut cwd = None;
    let mut terminal = None;
    let mut names: Vec<&str> = Vec::new();
    while let Some((flag, after)) = rest.split_first() {
        rest = match flag.as_str() {
            CWD_FLAG => {
                let (dir, after) = after.split_first()?;
                cwd = Some(PathBuf::from(dir));
                after
            }
            PASS_FLAG => {
                let (name, after) = after.split_first()?;
                names.push(name);
                after
            }
            TERMINAL_FLAG => {
                let (size, after) = parse_size(after)?;
                terminal = Some(size);
                after
            }
            END_OF_FLAGS => {
                rest = after;
                break;
            }
            _ => return None,
        };
    }
    let launch = match rest.split_first() {
        None => None,
        Some((program, args)) => Some(CloudLaunch {
            cwd: cwd?,
            program: program.clone(),
            args: args.to_vec(),
            env: passed_env(&names, &lookup),
            terminal,
        }),
    };
    Some(AttachArgs { id, from, launch })
}

pub(super) fn parse_size(words: &[String]) -> Option<(TerminalSize, &[String])> {
    let (cols, words) = words.split_first()?;
    let (rows, words) = words.split_first()?;
    let size = TerminalSize {
        cols: cols.parse().ok()?,
        rows: rows.parse().ok()?,
    };
    Some((size, words))
}

fn passed_env(names: &[&str], lookup: impl Fn(&str) -> Option<String>) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = Vec::new();
    for name in names.iter().copied().chain(ALWAYS_PASSED) {
        if env.iter().any(|(passed, _)| passed == name) {
            continue;
        }
        if let Some(value) = lookup(name) {
            env.push((name.to_owned(), value));
        }
    }
    env
}

pub(super) fn resolve_program(program: &str, path: Option<&str>) -> Option<String> {
    if program.contains(PATH_SEPARATOR) {
        return Some(program.to_owned());
    }
    std::env::split_paths(path?)
        .map(|dir| dir.join(program))
        .find(|candidate| is_executable(candidate))
        .map(|found| found.to_string_lossy().into_owned())
}

fn is_executable(path: &Path) -> bool {
    path.metadata().is_ok_and(|metadata| {
        metadata.is_file() && metadata.permissions().mode() & EXECUTABLE_BITS != 0
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<String> {
        text.split(' ').map(str::to_owned).collect()
    }

    fn lookup(name: &str) -> Option<String> {
        match name {
            "TOKEN" => Some("secret".to_owned()),
            "PATH" => Some("/usr/bin:/bin".to_owned()),
            "HOME" => Some("/home/me".to_owned()),
            _ => None,
        }
    }

    #[test]
    fn attach_arguments_name_the_process_offset_and_optional_launch() {
        let parsed = parse_attach(
            &words("chat-1 12 --cwd /repo --pass TOKEN --pass MISSING --pass HOME -- claude -p"),
            lookup,
        );
        assert_eq!(
            parsed.and_then(|args| args.launch.map(|launch| (args.from, launch))),
            Some((
                JournalOffset(12),
                CloudLaunch {
                    cwd: PathBuf::from("/repo"),
                    program: "claude".to_owned(),
                    args: words("-p"),
                    env: vec![
                        ("TOKEN".to_owned(), "secret".to_owned()),
                        ("HOME".to_owned(), "/home/me".to_owned()),
                        ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
                    ],
                    terminal: None,
                }
            ))
        );
        let reattach = parse_attach(&words("chat-1 0"), lookup);
        assert!(reattach.is_some_and(|args| args.launch.is_none()));
        assert!(parse_attach(&words("chat-1 0 -- claude"), lookup).is_none());
        assert!(parse_attach(&words("chat-1 x"), lookup).is_none());
        assert!(parse_attach(&words("chat-1 0 --bogus"), lookup).is_none());
    }

    #[test]
    fn a_terminal_launch_names_its_size() {
        let parsed = parse_attach(
            &words("shell-1 0 --cwd /repo --terminal 120 40 -- sh"),
            lookup,
        );
        assert_eq!(
            parsed.and_then(|args| args.launch?.terminal),
            Some(TerminalSize {
                cols: 120,
                rows: 40
            })
        );
        assert!(
            parse_attach(&words("shell-1 0 --cwd /repo --terminal 120 -- sh"), lookup).is_none()
        );
    }

    #[test]
    fn a_program_resolves_to_an_absolute_path_through_the_given_path() {
        assert_eq!(
            resolve_program("sh", Some("/nonexistent:/bin")),
            Some("/bin/sh".to_owned())
        );
        assert_eq!(resolve_program("./local", None), Some("./local".to_owned()));
        assert_eq!(resolve_program("krad-no-such-program", Some("/bin")), None);
        assert_eq!(resolve_program("sh", None), None);
    }
}
