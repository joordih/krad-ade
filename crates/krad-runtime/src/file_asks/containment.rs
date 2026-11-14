use krad_host::HostLink;
use krad_protocol::{Answer, ErrorCode};
use std::path::Path;

const OUTSIDE_ROOT: &str = "The file is not inside the caller's group folder";
const UNRESOLVED_ROOT: &str = "The caller's group folder could not be read";

fn refused(code: ErrorCode, message: &str) -> Answer {
    Answer::Failed {
        code,
        message: message.to_owned(),
    }
}

#[tracing::instrument(skip_all)]
pub(super) fn contained(link: &HostLink, root: &Path, path: &Path) -> Answer {
    let Ok(real_root) = link.canonicalize(root) else {
        return refused(ErrorCode::NotFound, UNRESOLVED_ROOT);
    };
    let mut existing = path;
    let mut rest = Vec::new();
    let real_existing = loop {
        if let Ok(real) = link.canonicalize(existing) {
            break real;
        }
        let (Some(parent), Some(name)) = (existing.parent(), existing.file_name()) else {
            return refused(ErrorCode::Forbidden, OUTSIDE_ROOT);
        };
        rest.push(name);
        existing = parent;
    };
    let real = rest
        .iter()
        .rev()
        .fold(real_existing, |real, name| real.join(name));
    if real.starts_with(&real_root) {
        Answer::Done
    } else {
        refused(ErrorCode::Forbidden, OUTSIDE_ROOT)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn a_link_that_leads_out_of_the_root_is_refused() -> std::io::Result<()> {
        let base =
            std::env::temp_dir().join(format!("krad-runtime-contained-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&base));
        let (root, outside) = (base.join("api"), base.join("secrets"));
        std::fs::create_dir_all(root.join("src"))?;
        std::fs::create_dir_all(&outside)?;
        std::fs::write(root.join("src/lib.rs"), "")?;
        std::fs::write(outside.join("key"), "")?;
        std::os::unix::fs::symlink(&outside, root.join("escape"))?;
        std::os::unix::fs::symlink(root.join("src/lib.rs"), root.join("lib.rs"))?;
        let link = HostLink::Local;
        let answer = |requested: &str| contained(&link, &root, &root.join(requested));
        for inside in ["src/lib.rs", "lib.rs", "src/new.rs"] {
            assert_eq!(answer(inside), Answer::Done, "{inside}");
        }
        for outside in ["escape/key", "escape/missing.rs"] {
            assert!(
                matches!(
                    answer(outside),
                    Answer::Failed {
                        code: ErrorCode::Forbidden,
                        ..
                    }
                ),
                "{outside}"
            );
        }
        std::fs::remove_dir_all(&base)
    }
}
