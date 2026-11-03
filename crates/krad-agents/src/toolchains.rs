#[cfg(feature = "python-toolchains")]
mod python;

use krad_host::toolchain_cli::{FOLDER_FLAG, PYTHON_SUBJECT, ROOT_FLAG, TOOLCHAINS_VERB};
use std::io::Write;
use std::path::PathBuf;

#[cfg(feature = "python-toolchains")]
const EXIT_OK: i32 = 0;
const EXIT_FAILED: i32 = 1;
const USAGE: &str = "usage: toolchains python --root <folder> [--folder <folder>]";
#[cfg(not(feature = "python-toolchains"))]
const NO_DETECTOR: &str = "this build cannot detect Python environments";

struct Request {
    root: PathBuf,
    folder: Option<PathBuf>,
}

fn request(args: &[String]) -> Option<Request> {
    let mut root = None;
    let mut folder = None;
    let mut words = args.iter();
    while let Some(word) = words.next() {
        match word.as_str() {
            ROOT_FLAG => root = words.next().map(PathBuf::from),
            FOLDER_FLAG => folder = words.next().map(PathBuf::from),
            _ => return None,
        }
    }
    Some(Request {
        root: root?,
        folder,
    })
}

pub(crate) fn run(args: &[String]) -> Option<i32> {
    let (verb, rest) = args.split_first()?;
    if verb != TOOLCHAINS_VERB {
        return None;
    }
    let Some((subject, flags)) = rest.split_first() else {
        return Some(report(USAGE));
    };
    if subject != PYTHON_SUBJECT {
        return Some(report(USAGE));
    }
    let Some(request) = request(flags) else {
        return Some(report(USAGE));
    };
    Some(answer(&request))
}

#[cfg(feature = "python-toolchains")]
fn answer(request: &Request) -> i32 {
    let found = python::environments(&request.root, request.folder.as_deref());
    match serde_json::to_vec(&found) {
        Ok(json) => {
            let mut out = std::io::stdout().lock();
            if out.write_all(&json).and_then(|()| out.flush()).is_ok() {
                EXIT_OK
            } else {
                EXIT_FAILED
            }
        }
        Err(error) => report(&error.to_string()),
    }
}

#[cfg(not(feature = "python-toolchains"))]
fn answer(request: &Request) -> i32 {
    let folder = request.folder.as_deref().unwrap_or(&request.root);
    report(&format!("{NO_DETECTOR} in {}", folder.display()))
}

fn report(message: &str) -> i32 {
    std::io::stderr()
        .write_all(format!("krad-ade-agents: {message}\n").as_bytes())
        .ok();
    EXIT_FAILED
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn flags_name_the_group_root_and_the_project_folder() {
        let parsed = request(&words("--root /repo --folder /repo/backend"));
        assert!(parsed
            .as_ref()
            .is_some_and(|request| request.root == std::path::Path::new("/repo")));
        assert!(parsed.is_some_and(|request| {
            request.folder.as_deref() == Some(std::path::Path::new("/repo/backend"))
        }));
        assert!(request(&words("--folder /repo")).is_none());
        assert!(request(&words("--root /repo --bogus")).is_none());
    }

    #[test]
    fn other_verbs_are_left_to_the_rest_of_the_cli() {
        assert_eq!(run(&words("send 3 hello")), None);
        assert_eq!(run(&words("toolchains ruby --root /x")), Some(EXIT_FAILED));
    }
}
