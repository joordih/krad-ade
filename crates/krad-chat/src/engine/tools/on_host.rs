use krad_host::HostCommand;
use std::path::Path;

use super::find::{glob, listed_files, MAX_FILES};
use super::grep::{cut, listed_matches, Query, MAX_MATCHES};
use super::{Ran, Workspace};

const LIST_SCRIPT: &str = r#"cd "$1" || exit 2; n="$2"
{ git ls-files -co --exclude-standard 2>/dev/null || find . -type f ! -path '*/.git/*' | sed 's|^\./||'; } | head -n "$n""#;
const GREP_SCRIPT: &str = r#"cd "$1" || exit 2; p="$2"; i="$3"; g="$4"; n="$5"; perl="$6"
if command -v rg >/dev/null 2>&1; then
  rg --line-number --no-heading --null --color never --hidden --glob '!.git' $i ${g:+--glob "$g"} -e "$p" . | head -n "$n"
elif grep -P -e x /dev/null >/dev/null 2>&1 || [ "$?" -eq 1 ]; then
  grep -rnIZ -P --exclude-dir=.git $i ${g:+--include="$g"} -e "$p" . | head -n "$n"
elif [ -n "$perl" ]; then
  exit 95
else
  grep -rnI --exclude-dir=.git $i ${g:+--include="$g"} -E -e "$p" . | head -n "$n"
fi"#;
const NEEDS_PERL_STATUS: i32 = 95;
const NEEDS_PERL_MARK: &str = "perl";
const NEEDS_PERL_TEXT: &str = "This host has neither ripgrep nor a grep with Perl-style patterns, so this pattern cannot run there. Write it without \\d, lazy quantifiers, inline flags or lookarounds, using classes such as [0-9] instead";
const PERL_ESCAPES: [char; 9] = ['d', 'D', 'h', 'H', 'x', 'p', 'P', 'A', 'z'];
const GROUP_OPEN: char = '(';
const GROUP_FLAG: char = '?';
const LAZY_AFTER: [char; 4] = ['*', '+', '?', '}'];
const ESCAPE: char = '\\';
const MAX_LISTED: usize = 200_000;
const PATH_END: char = '\0';
const IGNORE_CASE_FLAG: &str = "-i";
const CURRENT_DIR_PREFIX: &str = "./";
const FIELD_SEPARATOR: char = ':';
const NO_MATCH_STATUS: i32 = 1;
const FAILED_TEXT: &str = "The search failed on the host";

fn script(script: &str, root: &Path, rest: &[&str]) -> HostCommand {
    let mut command = HostCommand::posix_shell(script);
    command
        .arg(krad_host::remote_path(root))
        .args(rest)
        .in_login_shell();
    command
}

fn needs_perl(pattern: &str) -> bool {
    let mut previous: Option<char> = None;
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        if c == ESCAPE {
            if chars
                .next()
                .is_some_and(|escaped| PERL_ESCAPES.contains(&escaped))
            {
                return true;
            }
            previous = None;
            continue;
        }
        let lazy = c == GROUP_FLAG && previous.is_some_and(|p| LAZY_AFTER.contains(&p));
        let flagged = c == GROUP_FLAG && previous == Some(GROUP_OPEN);
        if lazy || flagged {
            return true;
        }
        previous = Some(c);
    }
    false
}

fn run(workspace: &Workspace, command: &HostCommand) -> Result<String, Ran> {
    let output = workspace
        .host
        .output(command)
        .map_err(|error| Ran::failed(format!("{FAILED_TEXT}: {error}")))?;
    let code = output.status.code();
    if code == Some(NEEDS_PERL_STATUS) {
        return Err(Ran::failed(NEEDS_PERL_TEXT.to_owned()));
    }
    let failed_silently = output.stdout.is_empty() && !output.stderr.is_empty();
    if (output.status.success() || code == Some(NO_MATCH_STATUS)) && !failed_silently {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    Err(Ran::failed(format!(
        "{FAILED_TEXT}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

fn relative(line: &str) -> &str {
    line.strip_prefix(CURRENT_DIR_PREFIX).unwrap_or(line)
}

pub(super) fn find(workspace: &Workspace, root: &Path, pattern: &str) -> Ran {
    let matcher = match glob(pattern) {
        Ok(matcher) => matcher,
        Err(error) => return Ran::failed(error),
    };
    let limit = MAX_LISTED.to_string();
    let listed = match run(workspace, &script(LIST_SCRIPT, root, &[&limit])) {
        Ok(listed) => listed,
        Err(failed) => return failed,
    };
    let mut paths: Vec<&str> = listed.lines().map(relative).collect();
    paths.sort_unstable();
    let mut matching = paths
        .into_iter()
        .filter(|path| matcher.is_match(path))
        .map(|path| workspace.shown(&root.join(path)));
    let found: Vec<String> = matching.by_ref().take(MAX_FILES).collect();
    let more = matching.next().is_some();
    listed_files(&found, more)
}

pub(super) fn grep(workspace: &Workspace, root: &Path, query: &Query) -> Ran {
    let ignore_case = if query.ignore_case {
        IGNORE_CASE_FLAG
    } else {
        ""
    };
    let command = script(
        GREP_SCRIPT,
        root,
        &[
            query.pattern,
            ignore_case,
            query.glob.unwrap_or_default(),
            &(MAX_MATCHES + 1).to_string(),
            if needs_perl(query.pattern) {
                NEEDS_PERL_MARK
            } else {
                ""
            },
        ],
    );
    let printed = match run(workspace, &command) {
        Ok(printed) => printed,
        Err(failed) => return failed,
    };
    let mut matches = printed
        .lines()
        .filter_map(|line| match_line(workspace, root, line));
    let found: Vec<String> = matches.by_ref().take(MAX_MATCHES).collect();
    let more = matches.next().is_some();
    listed_matches(&found, more)
}

fn match_line(workspace: &Workspace, root: &Path, line: &str) -> Option<String> {
    let (path, rest) = line.split_once(PATH_END).map_or_else(
        || relative(line).split_once(FIELD_SEPARATOR),
        |(path, rest)| Some((relative(path), rest)),
    )?;
    let (number, text) = rest.split_once(FIELD_SEPARATOR)?;
    let number: usize = number.parse().ok()?;
    Some(format!(
        "{}:{number}:{}",
        workspace.shown(&root.join(path)),
        cut(text)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn workspace() -> Workspace {
        Workspace {
            cwd: PathBuf::from("/srv/api"),
            env: Vec::new(),
            host: krad_host::HostLink::Local,
        }
    }

    #[test]
    fn a_match_from_the_host_is_shown_like_a_local_one() {
        let shown = match_line(
            &workspace(),
            Path::new("/srv/api"),
            "./src/main.rs:12:fn main() {",
        );
        assert_eq!(shown.as_deref(), Some("src/main.rs:12:fn main() {"));
        assert_eq!(
            match_line(&workspace(), Path::new("/srv/api"), "garbage"),
            None
        );
        assert_eq!(
            match_line(
                &workspace(),
                Path::new("/srv/api"),
                concat!("./a:b.txt", "\0", "3:x:y")
            )
            .as_deref(),
            Some("a:b.txt:3:x:y")
        );
    }

    #[test]
    fn perl_only_syntax_is_told_apart_from_plain_extended_patterns() {
        assert!(needs_perl(r"\d+"));
        assert!(needs_perl("a.*?b"));
        assert!(needs_perl("(?i)name"));
        assert!(needs_perl(r"\x41"));
        assert!(!needs_perl("fn [a-z_]+\\("));
        assert!(!needs_perl(r"\\d"));
        assert!(!needs_perl("(a|b)?"));
        assert!(!needs_perl("colou?r"));
    }

    #[test]
    fn the_host_scripts_take_the_folder_and_pattern_as_arguments() {
        let command = script(GREP_SCRIPT, Path::new("/srv/api"), &["a'b", "-i", "", "51"]);
        let remote = command.posix_script();
        assert!(remote.contains("'/srv/api'"));
        assert!(!remote.contains("a'b"));
    }
}
