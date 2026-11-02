use std::path::{Component, Path, PathBuf};

use crate::link::HostLink;

const INCLUDE_KEYWORD: &str = "include";
const COMMENT_MARK: char = '#';
const OPTION_SEPARATOR: char = '=';
const HOME_PREFIX: &str = "~/";
const MAX_INCLUDE_DEPTH: usize = 16;
const ANY_RUN: char = '*';
const ANY_ONE: char = '?';
const QUOTE: char = '"';

#[must_use]
pub fn read_ssh_config(host: &HostLink, home: &Path) -> String {
    let mut text = String::new();
    append_file(
        host,
        home,
        &krad_domain::ssh_config_file(home),
        0,
        &mut text,
    );
    text
}

fn append_file(host: &HostLink, home: &Path, file: &Path, depth: usize, text: &mut String) {
    let Ok(config) = host.read_to_string(file) else {
        return;
    };
    for line in config.lines() {
        match include_value(line) {
            Some(value) if depth < MAX_INCLUDE_DEPTH => {
                for pattern in value
                    .split_whitespace()
                    .map(|word| word.trim_matches(QUOTE))
                {
                    for included in expand(host, home, pattern) {
                        append_file(host, home, &included, depth + 1, text);
                    }
                }
            }
            Some(_) => {}
            None => {
                text.push_str(line);
                text.push('\n');
            }
        }
    }
}

fn include_value(line: &str) -> Option<&str> {
    let line = line.trim();
    if line.starts_with(COMMENT_MARK) {
        return None;
    }
    let (keyword, value) = line.split_once(|c: char| c.is_whitespace() || c == OPTION_SEPARATOR)?;
    keyword
        .eq_ignore_ascii_case(INCLUDE_KEYWORD)
        .then(|| value.trim().trim_start_matches(OPTION_SEPARATOR).trim())
}

fn include_path(home: &Path, pattern: &str) -> PathBuf {
    if let Some(rest) = pattern.strip_prefix(HOME_PREFIX) {
        return home.join(rest);
    }
    let path = Path::new(pattern);
    if path.has_root() {
        return path.to_path_buf();
    }
    krad_domain::ssh_dir(home).join(path)
}

fn expand(host: &HostLink, home: &Path, pattern: &str) -> Vec<PathBuf> {
    let path = include_path(home, pattern);
    let Some(name) = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
    else {
        return Vec::new();
    };
    if !name.contains([ANY_RUN, ANY_ONE]) {
        return vec![path];
    }
    let Some(dir) = path.parent() else {
        return Vec::new();
    };
    if dir.components().any(|part| {
        matches!(part, Component::Normal(part) if part.to_string_lossy().contains([ANY_RUN, ANY_ONE]))
    }) {
        return Vec::new();
    }
    let mut found: Vec<PathBuf> = host.read_dir(dir).map_or_else(
        |_| Vec::new(),
        |entries| {
            entries
                .into_iter()
                .filter(|entry| !entry.metadata.is_dir() && glob_matches(&name, &entry.name))
                .map(|entry| entry.path)
                .collect()
        },
    );
    found.sort();
    found
}

fn glob_matches(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();
    let (mut p, mut n) = (0, 0);
    let mut back: Option<(usize, usize)> = None;
    while n < name.len() {
        match pattern.get(p) {
            Some(&ANY_RUN) => {
                back = Some((p, n));
                p += 1;
            }
            Some(&c) if c == ANY_ONE || c == name[n] => {
                p += 1;
                n += 1;
            }
            _ => match back {
                Some((star, from)) => {
                    p = star + 1;
                    n = from + 1;
                    back = Some((star, from + 1));
                }
                None => return false,
            },
        }
    }
    pattern[p..].iter().all(|&c| c == ANY_RUN)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn globs_match_runs_and_single_characters() {
        assert!(glob_matches("*", "work"));
        assert!(glob_matches("*.conf", "gpu.conf"));
        assert!(!glob_matches("*.conf", "gpu.conf.bak"));
        assert!(glob_matches("host-?", "host-1"));
        assert!(!glob_matches("host-?", "host-12"));
        assert!(glob_matches("a*b*c", "aXXbYYc"));
    }

    #[test]
    fn include_lines_are_recognised_with_either_separator() {
        assert_eq!(include_value("Include config.d/*"), Some("config.d/*"));
        assert_eq!(include_value("  include=~/x"), Some("~/x"));
        assert_eq!(include_value("# Include nope"), None);
        assert_eq!(include_value("Host include"), None);
    }

    #[test]
    fn relative_includes_live_under_the_ssh_folder() {
        let home = Path::new("/home/me");
        assert_eq!(
            include_path(home, "config.d/a"),
            PathBuf::from("/home/me/.ssh/config.d/a")
        );
        assert_eq!(
            include_path(home, "~/other"),
            PathBuf::from("/home/me/other")
        );
        assert_eq!(
            include_path(home, "/etc/ssh/extra"),
            PathBuf::from("/etc/ssh/extra")
        );
    }

    #[test]
    fn included_files_are_read_in_place_and_in_order() -> TestResult {
        let home = std::env::temp_dir().join(format!("krad-ssh-config-{}", std::process::id()));
        let ssh = krad_domain::ssh_dir(&home);
        fs::create_dir_all(ssh.join("config.d"))?;
        fs::write(ssh.join("config"), "Include config.d/*\nHost main\n")?;
        fs::write(ssh.join("config.d/b"), "Host second\n")?;
        fs::write(
            ssh.join("config.d/a"),
            "Host first\nInclude ~/.ssh/config.d/a\n",
        )?;
        let text = read_ssh_config(&HostLink::Local, &home);
        assert_eq!(
            krad_domain::ssh_config_aliases(&text),
            ["first", "second", "main"]
        );
        fs::remove_dir_all(home)?;
        Ok(())
    }
}
