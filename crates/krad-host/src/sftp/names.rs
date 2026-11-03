const PATH_SEPARATOR: char = '/';
const ROOT: &str = "/";
const CURRENT_DIR: &str = ".";

#[must_use]
pub fn join(dir: &str, name: &str) -> String {
    if dir.ends_with(PATH_SEPARATOR) {
        format!("{dir}{name}")
    } else {
        format!("{dir}{PATH_SEPARATOR}{name}")
    }
}

pub(crate) fn parent(path: &str) -> &str {
    match path
        .trim_end_matches(PATH_SEPARATOR)
        .rsplit_once(PATH_SEPARATOR)
    {
        Some(("", _)) => ROOT,
        Some((dir, _)) => dir,
        None => CURRENT_DIR,
    }
}

pub(crate) fn file_name(path: &str) -> &str {
    path.trim_end_matches(PATH_SEPARATOR)
        .rsplit_once(PATH_SEPARATOR)
        .map_or(path, |(_, name)| name)
}
