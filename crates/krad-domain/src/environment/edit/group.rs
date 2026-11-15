use super::Edited;
use crate::environment::EnvironmentData;
use crate::group::WorktreeOrigin;
use crate::ids::GroupId;
use crate::toolchain::GroupActivation;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum GroupEdit {
    Add {
        group: GroupId,
        root: PathBuf,
        worktree: Option<WorktreeOrigin>,
    },
    Remove {
        group: GroupId,
    },
    Rename {
        group: GroupId,
        name: String,
    },
    SetActivation {
        group: GroupId,
        activation: GroupActivation,
    },
    #[cfg_attr(feature = "ts", ts(skip))]
    #[serde(other)]
    Unknown,
}

impl GroupEdit {
    pub(super) const fn group(&self) -> Option<GroupId> {
        match self {
            Self::Rename { group, .. } | Self::SetActivation { group, .. } => Some(*group),
            Self::Add { .. } | Self::Remove { .. } | Self::Unknown => None,
        }
    }

    pub(super) fn apply_to(&self, data: &mut EnvironmentData) -> Edited {
        match self {
            Self::Add {
                group,
                root,
                worktree,
            } => Edited::of(data.insert_group(*group, root.clone(), worktree.clone())),
            Self::Remove { group } => Edited::of(data.remove_group(*group).is_some()),
            Self::Rename { group, name } => {
                let Some(found) = data.groups.iter_mut().find(|found| found.id == *group) else {
                    return Edited::Ignored;
                };
                name.clone_into(&mut found.name);
                Edited::Applied
            }
            Self::SetActivation { group, activation } => {
                let Some(found) = data.groups.iter_mut().find(|found| found.id == *group) else {
                    return Edited::Ignored;
                };
                found.activation = *activation;
                Edited::Applied
            }
            Self::Unknown => Edited::Ignored,
        }
    }
}
