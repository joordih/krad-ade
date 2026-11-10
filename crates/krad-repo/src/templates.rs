use krad_domain::{SessionTemplate, TemplateError, TemplateId, TEMPLATES_DIR, TEMPLATE_EXTENSION};
use krad_host::{FileKind, HostLink};
use std::path::{Path, PathBuf};

use crate::RepoError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredTemplate {
    pub id: TemplateId,
    pub template: Result<SessionTemplate, TemplateError>,
}

#[must_use]
pub fn templates_dir(root: &Path) -> PathBuf {
    root.join(TEMPLATES_DIR)
}

fn template_id(path: &Path) -> Option<TemplateId> {
    let is_template = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case(TEMPLATE_EXTENSION));
    is_template
        .then(|| path.file_stem()?.to_str()?.parse().ok())
        .flatten()
}

/// # Errors
///
/// Returns [`RepoError::Io`] or [`RepoError::Host`] when the templates folder
/// exists but it or one of its files cannot be read. A file that is not a
/// valid template is listed with its error instead.
pub fn read_session_templates(
    host: &HostLink,
    root: &Path,
) -> Result<Vec<StoredTemplate>, RepoError> {
    let dir = templates_dir(root);
    let is_dir = host
        .metadata(&dir)
        .is_ok_and(|metadata| metadata.kind == FileKind::Directory);
    if !is_dir {
        return Ok(Vec::new());
    }
    let mut templates = Vec::new();
    for entry in host.read_dir(&dir)? {
        if entry.metadata.kind != FileKind::File {
            continue;
        }
        let Some(id) = template_id(&entry.path) else {
            continue;
        };
        let template = host.read_to_string(&entry.path)?.parse();
        templates.push(StoredTemplate { id, template });
    }
    templates.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(templates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_root(name: &str) -> Result<PathBuf, RepoError> {
        let root =
            std::env::temp_dir().join(format!("krad-templates-{name}-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root)?;
        }
        fs::create_dir_all(&root)?;
        Ok(root)
    }

    #[test]
    fn a_group_without_templates_lists_none() -> Result<(), RepoError> {
        let root = temp_root("none")?;
        let listed = read_session_templates(&HostLink::Local, &root);
        fs::remove_dir_all(&root)?;
        assert_eq!(listed?, Vec::new());
        Ok(())
    }

    #[test]
    fn templates_are_read_from_the_group_folder_in_name_order() -> Result<(), RepoError> {
        let root = temp_root("listed")?;
        let dir = templates_dir(&root);
        fs::create_dir_all(dir.join("nested.json"))?;
        fs::write(
            dir.join("review.json"),
            r#"{"format": 1, "name": "Review", "harness": "Codex"}"#,
        )?;
        fs::write(
            dir.join("fix-issue.json"),
            r#"{"format": 1, "name": "Fix", "worktree": {}}"#,
        )?;
        fs::write(dir.join("broken.json"), r#"{"format": 1}"#)?;
        fs::write(dir.join("notes.md"), "not a template")?;
        let listed = read_session_templates(&HostLink::Local, &root);
        fs::remove_dir_all(&root)?;
        let listed = listed?;
        let ids: Vec<&str> = listed.iter().map(|stored| stored.id.as_str()).collect();
        assert_eq!(ids, ["broken", "fix-issue", "review"]);
        assert!(listed[0].template.is_err());
        let names: Vec<&str> = listed
            .iter()
            .filter_map(|stored| stored.template.as_ref().ok())
            .map(|template| template.name.as_str())
            .collect();
        assert_eq!(names, ["Fix", "Review"]);
        Ok(())
    }
}
