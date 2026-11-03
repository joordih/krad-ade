use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::{ProjectFolder, Toolchain, ToolchainFamilyId};
use crate::host::Host;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ToolchainScope {
    #[serde(default, skip_serializing_if = "Host::is_local")]
    pub host: Host,
    pub group_root: PathBuf,
    #[serde(default, skip_serializing_if = "ProjectFolder::is_root")]
    pub project_folder: ProjectFolder,
    pub family: ToolchainFamilyId,
}

impl ToolchainScope {
    #[must_use]
    pub fn folder(&self) -> PathBuf {
        self.project_folder.inside(&self.group_root)
    }

    #[must_use]
    pub fn belongs_to(&self, host: &Host, group_root: &Path) -> bool {
        self.host == *host && self.group_root == group_root
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum ToolchainSelection {
    #[default]
    Auto,
    Chosen(Box<Toolchain>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolchainChoice {
    pub scope: ToolchainScope,
    pub toolchain: Toolchain,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ToolchainChoices(Vec<ToolchainChoice>);

impl ToolchainChoices {
    #[must_use]
    pub fn selection(&self, scope: &ToolchainScope) -> ToolchainSelection {
        self.0
            .iter()
            .find(|choice| choice.scope == *scope)
            .map_or(ToolchainSelection::Auto, |choice| {
                ToolchainSelection::Chosen(Box::new(choice.toolchain.clone()))
            })
    }

    pub fn choose(&mut self, scope: ToolchainScope, toolchain: Toolchain) {
        self.forget(&scope);
        self.0.push(ToolchainChoice { scope, toolchain });
    }

    pub fn forget(&mut self, scope: &ToolchainScope) {
        self.0.retain(|choice| choice.scope != *scope);
    }

    pub fn forget_group(&mut self, host: &Host, group_root: &Path) {
        self.0
            .retain(|choice| !choice.scope.belongs_to(host, group_root));
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EffectiveToolchain {
    Automatic(Option<Toolchain>),
    Chosen(Toolchain),
    ChosenMissing {
        chosen: Box<Toolchain>,
        fallback: Option<Toolchain>,
    },
}

impl EffectiveToolchain {
    #[must_use]
    pub fn resolve(selection: &ToolchainSelection, candidates: &[Toolchain]) -> Self {
        let best = candidates.first().cloned();
        match selection {
            ToolchainSelection::Auto => Self::Automatic(best),
            ToolchainSelection::Chosen(chosen) => candidates
                .iter()
                .find(|candidate| candidate.is_same(chosen))
                .map_or_else(
                    || Self::ChosenMissing {
                        chosen: chosen.clone(),
                        fallback: best,
                    },
                    |found| Self::Chosen(found.clone()),
                ),
        }
    }

    #[must_use]
    pub const fn toolchain(&self) -> Option<&Toolchain> {
        match self {
            Self::Automatic(found)
            | Self::ChosenMissing {
                fallback: found, ..
            } => found.as_ref(),
            Self::Chosen(chosen) => Some(chosen),
        }
    }

    #[must_use]
    pub const fn is_chosen(&self) -> bool {
        matches!(self, Self::Chosen(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toolchain::{LocatorId, ToolchainKind, ToolchainLocation};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn scope(root: &str, folder: &str) -> Result<ToolchainScope, Box<dyn std::error::Error>> {
        Ok(ToolchainScope {
            host: Host::Ssh {
                destination: "devbox".parse()?,
            },
            group_root: PathBuf::from(root),
            project_folder: ProjectFolder::try_from(PathBuf::from(folder))?,
            family: ToolchainFamilyId::new("python"),
        })
    }

    fn python(path: &str) -> Toolchain {
        Toolchain {
            family: ToolchainFamilyId::new("python"),
            kind: ToolchainKind::new("venv"),
            name: path.to_owned(),
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
    fn each_project_folder_keeps_its_own_choice() -> TestResult {
        let mut choices = ToolchainChoices::default();
        let backend = scope("/srv/repo", "backend")?;
        let ml = scope("/srv/repo", "ml")?;
        let venv = python("/srv/repo/backend/.venv/bin/python");
        choices.choose(backend.clone(), venv.clone());
        assert_eq!(
            choices.selection(&backend),
            ToolchainSelection::Chosen(Box::new(venv))
        );
        assert_eq!(choices.selection(&ml), ToolchainSelection::Auto);

        let json = serde_json::to_string(&choices)?;
        let back: ToolchainChoices = serde_json::from_str(&json)?;
        assert_eq!(back, choices);

        choices.forget(&backend);
        assert!(choices.is_empty());
        Ok(())
    }

    #[test]
    fn removing_a_group_drops_only_its_choices() -> TestResult {
        let mut choices = ToolchainChoices::default();
        let kept = scope("/srv/other", "")?;
        choices.choose(scope("/srv/repo", "backend")?, python("/a"));
        choices.choose(scope("/srv/repo", "")?, python("/b"));
        choices.choose(kept.clone(), python("/c"));
        choices.forget_group(&kept.host, Path::new("/srv/repo"));
        assert_eq!(
            choices.selection(&kept),
            ToolchainSelection::Chosen(Box::new(python("/c")))
        );
        assert_eq!(
            choices.selection(&scope("/srv/repo", "backend")?),
            ToolchainSelection::Auto
        );
        Ok(())
    }

    #[test]
    fn a_chosen_toolchain_wins_while_it_is_still_found() {
        let candidates = [python("/best"), python("/chosen")];
        let chosen = ToolchainSelection::Chosen(Box::new(python("/chosen")));
        assert_eq!(
            EffectiveToolchain::resolve(&chosen, &candidates),
            EffectiveToolchain::Chosen(python("/chosen"))
        );
        assert_eq!(
            EffectiveToolchain::resolve(&ToolchainSelection::Auto, &candidates).toolchain(),
            Some(&python("/best"))
        );
    }

    #[test]
    fn a_vanished_choice_falls_back_to_the_best_candidate() {
        let candidates = [python("/best")];
        let chosen = ToolchainSelection::Chosen(Box::new(python("/gone")));
        let effective = EffectiveToolchain::resolve(&chosen, &candidates);
        assert_eq!(
            effective,
            EffectiveToolchain::ChosenMissing {
                chosen: Box::new(python("/gone")),
                fallback: Some(python("/best")),
            }
        );
        assert_eq!(effective.toolchain(), Some(&python("/best")));
        assert!(!effective.is_chosen());
        assert_eq!(
            EffectiveToolchain::resolve(&ToolchainSelection::Auto, &[]).toolchain(),
            None
        );
    }
}
