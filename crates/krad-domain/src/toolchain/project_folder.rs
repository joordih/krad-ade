use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "PathBuf", into = "PathBuf")]
pub struct ProjectFolder(PathBuf);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotInsideTheGroup;

impl fmt::Display for NotInsideTheGroup {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("A project folder is a folder inside the group")
    }
}

impl std::error::Error for NotInsideTheGroup {}

impl ProjectFolder {
    #[must_use]
    pub fn root() -> Self {
        Self(PathBuf::new())
    }

    #[must_use]
    pub fn is_root(&self) -> bool {
        self.0.as_os_str().is_empty()
    }

    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    #[must_use]
    pub fn inside(&self, group_root: &Path) -> PathBuf {
        group_root.join(&self.0)
    }

    /// # Errors
    pub fn of(file: &Path, group_root: &Path) -> Result<Self, NotInsideTheGroup> {
        let relative = file
            .strip_prefix(group_root)
            .map_err(|_| NotInsideTheGroup)?;
        Self::try_from(relative.to_path_buf())
    }

    #[must_use]
    pub fn nearest(file: &Self, marked: &BTreeSet<Self>) -> Self {
        file.0
            .ancestors()
            .map(Path::to_path_buf)
            .map(Self)
            .find(|folder| marked.contains(folder))
            .unwrap_or_default()
    }

    #[must_use]
    pub fn depth(&self) -> usize {
        self.0.components().count()
    }
}

impl TryFrom<PathBuf> for ProjectFolder {
    type Error = NotInsideTheGroup;

    fn try_from(path: PathBuf) -> Result<Self, Self::Error> {
        if path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
        {
            let normal: PathBuf = path
                .components()
                .filter(|component| matches!(component, Component::Normal(_)))
                .collect();
            return Ok(Self(normal));
        }
        Err(NotInsideTheGroup)
    }
}

impl From<ProjectFolder> for PathBuf {
    fn from(folder: ProjectFolder) -> Self {
        folder.0
    }
}

impl fmt::Display for ProjectFolder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0.display(), formatter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn folders_stay_inside_the_group() {
        assert!(ProjectFolder::try_from(PathBuf::from("backend/api")).is_ok());
        assert!(ProjectFolder::try_from(PathBuf::from("../elsewhere")).is_err());
        assert!(ProjectFolder::try_from(PathBuf::from("/etc")).is_err());
        assert!(ProjectFolder::of(Path::new("/srv/app/x.py"), Path::new("/srv/other")).is_err());
    }

    #[test]
    fn a_file_belongs_to_its_nearest_marked_folder() -> TestResult {
        let marked: BTreeSet<ProjectFolder> = ["backend", "backend/ml", "web"]
            .into_iter()
            .map(|folder| ProjectFolder::try_from(PathBuf::from(folder)))
            .collect::<Result<_, _>>()?;
        let file = ProjectFolder::of(
            Path::new("/repo/backend/ml/train/run.py"),
            Path::new("/repo"),
        )?;
        assert_eq!(
            ProjectFolder::nearest(&file, &marked).as_path(),
            Path::new("backend/ml")
        );
        let loose = ProjectFolder::of(Path::new("/repo/scripts/x.py"), Path::new("/repo"))?;
        assert!(ProjectFolder::nearest(&loose, &marked).is_root());
        Ok(())
    }
}
