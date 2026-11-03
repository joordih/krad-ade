use krad_domain::{DeclaredToolchain, ProjectHints, VersionRequirement};
use std::path::Path;

use super::manifest::{HintFormat, HintSpec};

const COMMENT: char = '#';
const ENGINES_POINTER: &str = "/engines/";
const RUST_TOOLCHAIN_TABLE: &str = "toolchain";
const RUST_CHANNEL_KEY: &str = "channel";
const VENV_PATH_KEY: &str = "venvPath";
const VENV_KEY: &str = "venv";
const TOOL_TABLE: &str = "tool";
const PYRIGHT_TABLES: [&str; 2] = ["basedpyright", "pyright"];
const TOML_EXTENSION: &str = "toml";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Hint {
    Version(VersionRequirement),
    Declared(DeclaredToolchain),
}

fn first_line(text: &str) -> Option<&str> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with(COMMENT))
}

fn version_or_name(text: &str) -> Option<Hint> {
    let line = first_line(text)?;
    Some(VersionRequirement::parse(line).map_or_else(
        || Hint::Declared(DeclaredToolchain::Name(line.to_owned())),
        Hint::Version,
    ))
}

fn tool_versions(text: &str, tool: &str) -> Option<Hint> {
    text.lines().find_map(|line| {
        let mut words = line.split_whitespace();
        (words.next()? == tool)
            .then(|| words.next())
            .flatten()
            .and_then(VersionRequirement::parse)
            .map(Hint::Version)
    })
}

fn package_json_engine(text: &str, engine: &str) -> Option<Hint> {
    let json: serde_json::Value = serde_json::from_str(text).ok()?;
    let range = json
        .pointer(&format!("{ENGINES_POINTER}{engine}"))?
        .as_str()?;
    VersionRequirement::parse(range).map(Hint::Version)
}

fn rust_toolchain(text: &str, file: &Path) -> Option<Hint> {
    let channel = if file.extension().is_some_and(|ext| ext == TOML_EXTENSION) {
        let table: toml::Table = text.parse().ok()?;
        table
            .get(RUST_TOOLCHAIN_TABLE)?
            .get(RUST_CHANNEL_KEY)?
            .as_str()?
            .to_owned()
    } else {
        first_line(text)?.to_owned()
    };
    Some(Hint::Declared(DeclaredToolchain::Name(channel)))
}

fn pyright_venv(text: &str, file: &Path, folder: &Path) -> Option<Hint> {
    let (venv_path, venv) = if file.extension().is_some_and(|ext| ext == TOML_EXTENSION) {
        let table: toml::Table = text.parse().ok()?;
        let tool = table.get(TOOL_TABLE)?;
        PYRIGHT_TABLES.iter().find_map(|name| {
            let section = tool.get(*name)?;
            Some((
                section.get(VENV_PATH_KEY)?.as_str()?.to_owned(),
                section.get(VENV_KEY)?.as_str()?.to_owned(),
            ))
        })?
    } else {
        let json: serde_json::Value = serde_json::from_str(text).ok()?;
        (
            json.get(VENV_PATH_KEY)?.as_str()?.to_owned(),
            json.get(VENV_KEY)?.as_str()?.to_owned(),
        )
    };
    Some(Hint::Declared(DeclaredToolchain::Path(
        folder.join(venv_path).join(venv),
    )))
}

pub(crate) fn read_hint(spec: &HintSpec, text: &str, file: &Path, folder: &Path) -> Option<Hint> {
    match &spec.format {
        HintFormat::VersionText => version_or_name(text),
        HintFormat::ToolVersions { tool } => tool_versions(text, tool),
        HintFormat::PackageJsonEngine { engine } => package_json_engine(text, engine),
        HintFormat::RustToolchain => rust_toolchain(text, file),
        HintFormat::EnvironmentName => {
            first_line(text).map(|name| Hint::Declared(DeclaredToolchain::Name(name.to_owned())))
        }
        HintFormat::PyrightVenv => pyright_venv(text, file, folder),
    }
}

pub(crate) fn merge(hints: impl IntoIterator<Item = Hint>) -> ProjectHints {
    let mut merged = ProjectHints::default();
    for hint in hints {
        match hint {
            Hint::Version(version) if merged.version.is_none() => merged.version = Some(version),
            Hint::Declared(declared) if merged.declared.is_none() => {
                merged.declared = Some(declared);
            }
            Hint::Version(_) | Hint::Declared(_) => {}
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn spec(format: HintFormat) -> HintSpec {
        HintSpec {
            file: String::new(),
            format,
        }
    }

    #[test]
    fn version_files_give_a_version_or_a_named_environment() {
        let folder = Path::new("/repo");
        let file = Path::new("/repo/.python-version");
        assert_eq!(
            read_hint(
                &spec(HintFormat::VersionText),
                "# pinned\n3.12\n",
                file,
                folder
            ),
            Some(Hint::Version(VersionRequirement::Prefix(vec![3, 12])))
        );
        assert_eq!(
            read_hint(&spec(HintFormat::VersionText), "ml-env\n", file, folder),
            Some(Hint::Declared(DeclaredToolchain::Name("ml-env".to_owned())))
        );
        assert_eq!(
            read_hint(&spec(HintFormat::VersionText), "lts/iron", file, folder),
            Some(Hint::Declared(DeclaredToolchain::Name(
                "lts/iron".to_owned()
            )))
        );
    }

    #[test]
    fn tool_versions_and_engines_pick_their_tool() {
        let folder = Path::new("/repo");
        let file = Path::new("/repo/x");
        let tools = HintFormat::ToolVersions {
            tool: "nodejs".to_owned(),
        };
        assert_eq!(
            read_hint(
                &spec(tools),
                "python 3.12.1\nnodejs 20.11.0\n",
                file,
                folder
            ),
            Some(Hint::Version(VersionRequirement::Prefix(vec![20, 11, 0])))
        );
        let engine = HintFormat::PackageJsonEngine {
            engine: "node".to_owned(),
        };
        assert_eq!(
            read_hint(
                &spec(engine),
                r#"{"engines":{"node":">=18.17"}}"#,
                file,
                folder
            ),
            Some(Hint::Version(VersionRequirement::AtLeast(vec![18, 17])))
        );
    }

    #[test]
    fn project_configuration_declares_an_environment() {
        let folder = Path::new("/repo/backend");
        let json = r#"{"venvPath": ".", "venv": ".venv-dev"}"#;
        assert_eq!(
            read_hint(
                &spec(HintFormat::PyrightVenv),
                json,
                Path::new("/repo/backend/pyrightconfig.json"),
                folder
            ),
            Some(Hint::Declared(DeclaredToolchain::Path(PathBuf::from(
                "/repo/backend/./.venv-dev"
            ))))
        );
        let pyproject = "[tool.basedpyright]\nvenvPath = \"/opt/envs\"\nvenv = \"api\"\n";
        assert_eq!(
            read_hint(
                &spec(HintFormat::PyrightVenv),
                pyproject,
                Path::new("/repo/backend/pyproject.toml"),
                folder
            ),
            Some(Hint::Declared(DeclaredToolchain::Path(PathBuf::from(
                "/opt/envs/api"
            ))))
        );
        let rust = "[toolchain]\nchannel = \"1.82.0\"\n";
        assert_eq!(
            read_hint(
                &spec(HintFormat::RustToolchain),
                rust,
                Path::new("/repo/rust-toolchain.toml"),
                folder
            ),
            Some(Hint::Declared(DeclaredToolchain::Name("1.82.0".to_owned())))
        );
    }

    #[test]
    fn the_first_hint_of_each_sort_wins() {
        let merged = merge([
            Hint::Declared(DeclaredToolchain::Name("a".to_owned())),
            Hint::Version(VersionRequirement::Prefix(vec![3])),
            Hint::Declared(DeclaredToolchain::Name("b".to_owned())),
        ]);
        assert_eq!(
            merged.declared,
            Some(DeclaredToolchain::Name("a".to_owned()))
        );
        assert_eq!(merged.version, Some(VersionRequirement::Prefix(vec![3])));
    }
}
