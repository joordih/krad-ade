use krad_host::HostLink;
use std::path::{Path, PathBuf};

const SEPARATOR: char = '/';
const WILDCARD: char = '*';
pub(crate) const MAX_GLOB_MATCHES: usize = 256;

pub(crate) fn wildcard_matches(pattern: &str, name: &str) -> bool {
    let mut pieces = pattern.split(WILDCARD);
    let Some(first) = pieces.next() else {
        return name.is_empty();
    };
    let Some(mut rest) = name.strip_prefix(first) else {
        return false;
    };
    let pieces: Vec<&str> = pieces.collect();
    let Some((last, middle)) = pieces.split_last() else {
        return rest.is_empty();
    };
    for piece in middle {
        match rest.find(piece) {
            Some(at) => rest = &rest[at + piece.len()..],
            None => return false,
        }
    }
    rest.len() >= last.len() && rest.ends_with(last)
}

pub(crate) fn expand(link: &HostLink, base: &Path, pattern: &str) -> Vec<PathBuf> {
    let mut current = vec![base.to_path_buf()];
    for segment in pattern
        .split(SEPARATOR)
        .filter(|segment| !segment.is_empty())
    {
        if !segment.contains(WILDCARD) {
            current = current.into_iter().map(|path| path.join(segment)).collect();
            continue;
        }
        let mut next = Vec::new();
        for dir in &current {
            let Ok(entries) = link.read_dir(dir) else {
                continue;
            };
            let mut names: Vec<String> = entries
                .into_iter()
                .map(|entry| entry.name)
                .filter(|name| wildcard_matches(segment, name))
                .collect();
            names.sort();
            next.extend(names.into_iter().map(|name| dir.join(name)));
            if next.len() >= MAX_GLOB_MATCHES {
                break;
            }
        }
        next.truncate(MAX_GLOB_MATCHES);
        current = next;
    }
    current.retain(|path| link.exists(path));
    current
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards_match_any_run_of_characters() {
        assert!(wildcard_matches("*", "anything"));
        assert!(wildcard_matches("python3*", "python3.12"));
        assert!(wildcard_matches("jdk-*-x64", "jdk-21.0.2-x64"));
        assert!(wildcard_matches("*.venv", ".venv"));
        assert!(!wildcard_matches("python3*", "python2.7"));
        assert!(!wildcard_matches("jdk-*-x64", "jdk-21-arm64"));
        assert!(wildcard_matches("plain", "plain"));
        assert!(!wildcard_matches("plain", "plainer"));
    }

    #[test]
    fn patterns_expand_over_the_folder_tree() -> std::io::Result<()> {
        let base = std::env::temp_dir().join(format!("krad-toolchain-glob-{}", std::process::id()));
        for version in ["v18.19.0", "v20.11.1"] {
            std::fs::create_dir_all(base.join("versions/node").join(version).join("bin"))?;
            std::fs::write(
                base.join("versions/node").join(version).join("bin/node"),
                "",
            )?;
        }
        std::fs::create_dir_all(base.join("versions/node/broken"))?;
        let found = expand(&HostLink::Local, &base, "versions/node/*/bin/node");
        assert_eq!(
            found,
            [
                base.join("versions/node/v18.19.0/bin/node"),
                base.join("versions/node/v20.11.1/bin/node"),
            ]
        );
        std::fs::remove_dir_all(base)
    }
}
