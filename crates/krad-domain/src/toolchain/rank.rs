use std::cmp::Reverse;
use std::path::{Path, PathBuf};

use super::{Toolchain, ToolchainKind, ToolchainLocation, VersionRequirement};

const NAME_QUALIFIER: char = '-';

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeclaredToolchain {
    Path(PathBuf),
    Name(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectHints {
    pub declared: Option<DeclaredToolchain>,
    pub version: Option<VersionRequirement>,
}

#[derive(Clone, Copy, Debug)]
pub struct RankContext<'a> {
    pub group_root: &'a Path,
    pub project_folder: &'a Path,
    pub priority: &'a [ToolchainKind],
    pub hints: &'a ProjectHints,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Distance {
    InProjectFolder,
    InGroup,
    Elsewhere,
}

fn is_declared(toolchain: &Toolchain, declared: Option<&DeclaredToolchain>) -> bool {
    match declared {
        Some(DeclaredToolchain::Path(path)) => {
            toolchain.prefix.as_deref() == Some(path.as_path())
                || toolchain.path().is_some_and(|own| own.starts_with(path))
        }
        Some(DeclaredToolchain::Name(name)) => {
            toolchain.name == *name
                || toolchain
                    .name
                    .strip_prefix(name.as_str())
                    .is_some_and(|rest| rest.starts_with(NAME_QUALIFIER))
                || toolchain
                    .prefix
                    .as_deref()
                    .and_then(Path::file_name)
                    .is_some_and(|folder| folder.to_string_lossy() == *name)
        }
        None => false,
    }
}

fn distance(toolchain: &Toolchain, context: &RankContext<'_>) -> Distance {
    let places = [
        toolchain.project.as_deref(),
        toolchain.prefix.as_deref(),
        toolchain.path(),
    ];
    let within = |folder: &Path| {
        places
            .iter()
            .flatten()
            .any(|place| place.starts_with(folder))
    };
    if within(context.project_folder) {
        Distance::InProjectFolder
    } else if within(context.group_root) {
        Distance::InGroup
    } else {
        Distance::Elsewhere
    }
}

fn kind_rank(kind: &ToolchainKind, priority: &[ToolchainKind]) -> usize {
    priority
        .iter()
        .position(|known| known == kind)
        .unwrap_or(priority.len())
}

#[must_use]
pub fn rank_toolchains(
    mut candidates: Vec<Toolchain>,
    context: &RankContext<'_>,
) -> Vec<Toolchain> {
    let mut unique: Vec<Toolchain> = Vec::with_capacity(candidates.len());
    for candidate in candidates.drain(..) {
        if !unique.iter().any(|known| known.is_same(&candidate)) {
            unique.push(candidate);
        }
    }
    unique.sort_by_cached_key(|toolchain| {
        let declared = is_declared(toolchain, context.hints.declared.as_ref());
        let wanted = context.hints.version.as_ref().is_none_or(|requirement| {
            toolchain
                .version
                .as_ref()
                .is_some_and(|version| requirement.matches(version))
        });
        (
            !declared,
            !wanted,
            distance(toolchain, context),
            kind_rank(&toolchain.kind, context.priority),
            matches!(toolchain.location, ToolchainLocation::Bundled),
            Reverse(toolchain.version.clone()),
            toolchain.location.path().map(Path::to_path_buf),
        )
    });
    unique
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toolchain::{LocatorId, ToolchainFamilyId};

    fn python(kind: &str, prefix: &str, version: &str) -> Toolchain {
        Toolchain {
            family: ToolchainFamilyId::new("python"),
            kind: ToolchainKind::new(kind),
            name: prefix.rsplit('/').next().unwrap_or_default().to_owned(),
            version: version.parse().ok(),
            location: ToolchainLocation::Path {
                path: PathBuf::from(format!("{prefix}/bin/python")),
            },
            prefix: Some(PathBuf::from(prefix)),
            project: None,
            locator: LocatorId::new("pet"),
        }
    }

    fn priority() -> Vec<ToolchainKind> {
        ["uv", "poetry", "venv", "conda", "pyenv", "system"]
            .into_iter()
            .map(ToolchainKind::new)
            .collect()
    }

    fn names(ranked: &[Toolchain]) -> Vec<&str> {
        ranked
            .iter()
            .map(|toolchain| toolchain.name.as_str())
            .collect()
    }

    #[test]
    fn the_project_folders_own_venv_wins_over_global_ones() {
        let priority = priority();
        let hints = ProjectHints::default();
        let context = RankContext {
            group_root: Path::new("/repo"),
            project_folder: Path::new("/repo/backend"),
            priority: &priority,
            hints: &hints,
        };
        let ranked = rank_toolchains(
            vec![
                python("system", "/usr", "3.10.12"),
                python("conda", "/home/me/miniconda3/envs/ml", "3.11.9"),
                python("venv", "/repo/.venv", "3.12.1"),
                python("venv", "/repo/backend/.venv", "3.12.4"),
            ],
            &context,
        );
        assert_eq!(names(&ranked), [".venv", ".venv", "ml", "usr"]);
        assert_eq!(
            ranked[0].prefix.as_deref(),
            Some(Path::new("/repo/backend/.venv"))
        );
    }

    #[test]
    fn a_declared_environment_and_a_requested_version_come_first() {
        let priority = priority();
        let declared = ProjectHints {
            declared: Some(DeclaredToolchain::Name("ml".to_owned())),
            version: None,
        };
        let context = RankContext {
            group_root: Path::new("/repo"),
            project_folder: Path::new("/repo"),
            priority: &priority,
            hints: &declared,
        };
        let candidates = vec![
            python("venv", "/repo/.venv", "3.12.1"),
            python("conda", "/home/me/miniconda3/envs/ml", "3.11.9"),
        ];
        assert_eq!(
            names(&rank_toolchains(candidates.clone(), &context))[0],
            "ml"
        );

        let wanted = ProjectHints {
            declared: None,
            version: VersionRequirement::parse("3.11"),
        };
        let context = RankContext {
            hints: &wanted,
            ..context
        };
        assert_eq!(names(&rank_toolchains(candidates, &context))[0], "ml");
    }

    #[test]
    fn duplicates_collapse_and_newer_versions_break_ties() {
        let priority = priority();
        let hints = ProjectHints::default();
        let context = RankContext {
            group_root: Path::new("/repo"),
            project_folder: Path::new("/repo"),
            priority: &priority,
            hints: &hints,
        };
        let ranked = rank_toolchains(
            vec![
                python("pyenv", "/home/me/.pyenv/versions/3.9.18", "3.9.18"),
                python("pyenv", "/home/me/.pyenv/versions/3.12.1", "3.12.1"),
                python("pyenv", "/home/me/.pyenv/versions/3.12.1", "3.12.1"),
            ],
            &context,
        );
        assert_eq!(names(&ranked), ["3.12.1", "3.9.18"]);
    }
}
