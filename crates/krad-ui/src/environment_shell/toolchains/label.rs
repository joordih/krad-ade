use krad_domain::{EffectiveToolchain, Toolchain};
use krad_extensions::ToolchainFamilyManifest;
use std::path::{Component, Path, PathBuf};

pub(super) const NO_FALLBACK_TEXT: &str = "no other toolchain was found";
pub(super) const USING_TEXT: &str = "using";
pub(super) const VANISHED_TEXT: &str = "is no longer found;";
const FINDING_PREFIX: &str = "Finding";
const ELLIPSIS: &str = "…";
const HOME_MARK: &str = "~";
const PATH_SEPARATOR: &str = "/";
const SHORT_PATH_TAIL: usize = 3;
const DETAIL_SEPARATOR: &str = " · ";
const PATH_MARK: char = '/';

pub(super) fn no_toolchain_label(family: &ToolchainFamilyManifest) -> String {
    format!("No {} toolchain", family.name)
}

fn detail(family: &ToolchainFamilyManifest, toolchain: &Toolchain) -> String {
    let name = toolchain.name.trim();
    if name.is_empty() || name == family.name || name.contains(PATH_MARK) {
        family.label(&toolchain.kind)
    } else {
        name.to_owned()
    }
}

pub(super) fn toolchain_title(family: &ToolchainFamilyManifest, toolchain: &Toolchain) -> String {
    let detail = detail(family, toolchain);
    match &toolchain.version {
        Some(version) => format!("{} {version} ({detail})", family.name),
        None => format!("{} ({detail})", family.name),
    }
}

pub(super) fn status_label(
    family: &ToolchainFamilyManifest,
    effective: Option<&EffectiveToolchain>,
    resolving: bool,
) -> String {
    match effective.and_then(EffectiveToolchain::toolchain) {
        Some(toolchain) => toolchain_title(family, toolchain),
        None if resolving && effective.is_none() => {
            format!("{FINDING_PREFIX} {}{ELLIPSIS}", family.name)
        }
        None => no_toolchain_label(family),
    }
}

pub(super) fn short_path(path: &Path, home: Option<&Path>) -> String {
    let (lead, rest): (Option<&str>, PathBuf) =
        match home.and_then(|home| path.strip_prefix(home).ok()) {
            Some(inside) => (Some(HOME_MARK), inside.to_path_buf()),
            None => (None, path.to_path_buf()),
        };
    let parts: Vec<String> = rest
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    if parts.len() > SHORT_PATH_TAIL + 1 {
        let tail = parts[parts.len() - SHORT_PATH_TAIL..].join(PATH_SEPARATOR);
        return match lead {
            Some(lead) => format!("{lead}{PATH_SEPARATOR}{ELLIPSIS}{PATH_SEPARATOR}{tail}"),
            None => format!("{ELLIPSIS}{PATH_SEPARATOR}{tail}"),
        };
    }
    let joined = parts.join(PATH_SEPARATOR);
    match lead {
        Some(lead) => format!("{lead}{PATH_SEPARATOR}{joined}"),
        None if path.has_root() => format!("{PATH_SEPARATOR}{joined}"),
        None => joined,
    }
}

pub(super) fn row_detail(toolchain: &Toolchain, home: Option<&Path>) -> String {
    let version = toolchain.version.as_ref().map(ToString::to_string);
    let path = toolchain.path().map(|path| short_path(path, home));
    [version, path]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(DETAIL_SEPARATOR)
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{LocatorId, ToolchainFamilyId, ToolchainKind, ToolchainLocation};
    use krad_extensions::toolchain_manifest::KindLabel;

    fn family(kinds: &[(&str, &str)]) -> ToolchainFamilyManifest {
        ToolchainFamilyManifest {
            id: ToolchainFamilyId::new("python"),
            name: "Python".to_owned(),
            languages: vec!["python".to_owned()],
            markers: Vec::new(),
            priority: Vec::new(),
            kinds: kinds
                .iter()
                .map(|(id, label)| KindLabel {
                    id: ToolchainKind::new(*id),
                    label: (*label).to_owned(),
                })
                .collect(),
            locators: Vec::new(),
            hints: Vec::new(),
            activation: Vec::new(),
            entered: Vec::new(),
        }
    }

    fn toolchain(kind: &str, name: &str, path: &str) -> Toolchain {
        Toolchain {
            family: ToolchainFamilyId::new("python"),
            kind: ToolchainKind::new(kind),
            name: name.to_owned(),
            version: "3.12.4".parse().ok(),
            location: ToolchainLocation::Path {
                path: PathBuf::from(path),
            },
            prefix: None,
            project: None,
            locator: LocatorId::new("pet"),
        }
    }

    #[test]
    fn the_status_names_the_family_version_and_environment() {
        let family = family(&[("system", "System"), ("venv", "Virtual environment")]);
        let venv = toolchain("venv", ".venv", "/srv/app/.venv/bin/python");
        assert_eq!(toolchain_title(&family, &venv), "Python 3.12.4 (.venv)");
        let system = toolchain("system", "/usr/bin/python3", "/usr/bin/python3");
        assert_eq!(toolchain_title(&family, &system), "Python 3.12.4 (System)");
        let chosen = EffectiveToolchain::Automatic(Some(venv));
        assert_eq!(
            status_label(&family, Some(&chosen), false),
            "Python 3.12.4 (.venv)"
        );
        assert_eq!(status_label(&family, None, true), "Finding Python…");
        assert_eq!(
            status_label(&family, Some(&EffectiveToolchain::Automatic(None)), false),
            "No Python toolchain"
        );
    }

    #[test]
    fn long_paths_keep_their_tail_and_home_becomes_a_tilde() {
        let home = Path::new("/home/me");
        assert_eq!(
            short_path(Path::new("/home/me/work/api/.venv/bin/python"), Some(home)),
            "~/…/.venv/bin/python"
        );
        assert_eq!(
            short_path(Path::new("/home/me/.venv/bin/python"), Some(home)),
            "~/.venv/bin/python"
        );
        assert_eq!(
            short_path(Path::new("/usr/bin/python3"), None),
            "/usr/bin/python3"
        );
        assert_eq!(
            row_detail(
                &toolchain("venv", ".venv", "/home/me/.venv/bin/python"),
                Some(home)
            ),
            "3.12.4 · ~/.venv/bin/python"
        );
    }
}
