use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::error::ExtensionError;

const OPEN: &str = "{{";
const CLOSE: &str = "}}";
const PIPE: char = '|';
const QUOTE: char = '"';
const STRIP_PREFIX: &str = "strip_prefix";
const GLOB: &str = "glob";
const QUOTE_FILTER: &str = "quote";
const WILDCARD: char = '*';
const SEPARATOR: char = '/';
pub(crate) const SERVER_RUNTIME_VAR: &str = "server_runtime";
pub(crate) const SERVER_RUNTIME_DIR_VAR: &str = "server_runtime_dir";
pub(crate) const SERVER_RUNTIME_PREFIX: &str = "server_runtime.";
const LEGACY_SERVER_RUNTIME_VAR: &str = "toolchain";
const LEGACY_SERVER_RUNTIME_DIR_VAR: &str = "toolchain_dir";
const LEGACY_SERVER_RUNTIME_PREFIX: &str = "toolchain.";

#[derive(Clone, Debug, Default)]
pub(crate) struct Vars {
    values: BTreeMap<String, String>,
}

impl Vars {
    pub(crate) fn set(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.values.insert(name.into(), value.into());
    }

    pub(crate) fn get(&self, name: &str) -> Option<&str> {
        self.values
            .get(name)
            .or_else(|| self.values.get(current_name(name).as_ref()))
            .map(String::as_str)
    }

    pub(crate) fn with(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.set(name, value);
        self
    }
}

fn current_name(name: &str) -> std::borrow::Cow<'_, str> {
    if name == LEGACY_SERVER_RUNTIME_VAR {
        return SERVER_RUNTIME_VAR.into();
    }
    if name == LEGACY_SERVER_RUNTIME_DIR_VAR {
        return SERVER_RUNTIME_DIR_VAR.into();
    }
    match name.strip_prefix(LEGACY_SERVER_RUNTIME_PREFIX) {
        Some(id) => format!("{SERVER_RUNTIME_PREFIX}{id}").into(),
        None => name.into(),
    }
}

fn unquote(text: &str) -> Option<&str> {
    text.strip_prefix(QUOTE)?.strip_suffix(QUOTE)
}

fn glob(pattern: &str, dir: &Path) -> Option<String> {
    let (folder, file) = pattern.rsplit_once(SEPARATOR).unwrap_or(("", pattern));
    let (before, after) = file.split_once(WILDCARD)?;
    let parent = dir.join(folder);
    fs::read_dir(&parent)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .find(|name| {
            name.starts_with(before) && name.ends_with(after) && name.len() >= file.len() - 1
        })
        .map(|name| parent.join(name).to_string_lossy().into_owned())
}

fn expression(text: &str, vars: &Vars, dir: &Path) -> Option<String> {
    let mut parts = text.split(PIPE).map(str::trim);
    let head = parts.next()?;
    let mut value = match unquote(head) {
        Some(literal) => literal.to_owned(),
        None => vars.get(head)?.to_owned(),
    };
    for filter in parts {
        let (name, argument) = filter.split_once(' ').unwrap_or((filter, ""));
        value = match name {
            STRIP_PREFIX => {
                let prefix = unquote(argument.trim())?;
                value.strip_prefix(prefix).unwrap_or(&value).to_owned()
            }
            GLOB => glob(&value, dir)?,
            QUOTE_FILTER => krad_host::quote(&value),
            _ => return None,
        };
    }
    Some(value)
}

pub(crate) fn render(text: &str, vars: &Vars, dir: &Path) -> Result<String, ExtensionError> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(OPEN) {
        out.push_str(&rest[..start]);
        let after = &rest[start + OPEN.len()..];
        let end = after
            .find(CLOSE)
            .ok_or_else(|| ExtensionError::Template(text.to_owned()))?;
        let filled = expression(&after[..end], vars, dir)
            .ok_or_else(|| ExtensionError::Template(text.to_owned()))?;
        out.push_str(&filled);
        rest = &after[end + CLOSE.len()..];
    }
    out.push_str(rest);
    Ok(out)
}

pub(crate) fn render_all(
    texts: &[String],
    vars: &Vars,
    dir: &Path,
) -> Result<Vec<String>, ExtensionError> {
    texts.iter().map(|text| render(text, vars, dir)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variables_and_mason_filters_fill_a_template() -> Result<(), ExtensionError> {
        let vars = Vars::default().with("version", "kotlin-lsp/v263.1");
        let dir = Path::new("/nowhere");
        let url = render(
            "https://x/{{ version | strip_prefix \"kotlin-lsp/v\" }}/k-{{version}}.sit",
            &vars,
            dir,
        )?;
        assert_eq!(url, "https://x/263.1/k-kotlin-lsp/v263.1.sit");
        assert!(render("{{missing}}", &vars, dir).is_err());
        assert_eq!(render("plain", &vars, dir)?, "plain");
        Ok(())
    }

    #[test]
    fn a_glob_finds_the_one_file_that_fits() -> Result<(), ExtensionError> {
        let dir = std::env::temp_dir().join(format!("krad-glob-{}", std::process::id()));
        fs::create_dir_all(dir.join("plugins"))?;
        fs::write(dir.join("plugins/org.launcher_1.2.jar"), "")?;
        let found = render(
            "{{ \"plugins/org.launcher_*.jar\" | glob }}",
            &Vars::default(),
            &dir,
        )?;
        assert!(Path::new(&found).ends_with(Path::new("plugins").join("org.launcher_1.2.jar")));
        fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
