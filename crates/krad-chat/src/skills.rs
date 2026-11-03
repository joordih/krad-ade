use krad_domain::{parse_wsl_path, wsl_unc_path, WslPath};
use krad_host::HostLink;
use serde_json::Value;
use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub(crate) const CLAUDE_DIR: &str = ".claude";
const AGENTS_DIR: &str = ".agents";
const LINUX_ROOT: char = '/';
const SKILLS_DIR: &str = "skills";
const COMMANDS_DIR: &str = "commands";
const SKILL_FILE: &str = "SKILL.md";
const COMMAND_EXTENSION: &str = "md";
const SETTINGS_FILE: &str = "settings.json";
const PLUGINS_DIR: &str = "plugins";
const INSTALLED_PLUGINS_FILE: &str = "installed_plugins.json";
const ENABLED_PLUGINS_KEY: &str = "enabledPlugins";
const PLUGINS_KEY: &str = "plugins";
const INSTALL_PATH_KEY: &str = "installPath";
const MARKETPLACE_SEPARATOR: char = '@';
const NAMESPACE_SEPARATOR: &str = ":";
const FRONT_MATTER_FENCE: &str = "---";
const NAME_KEY: &str = "name";
const DESCRIPTION_KEY: &str = "description";
const BLOCK_SCALARS: [&str; 6] = [">", ">-", ">+", "|", "|-", "|+"];
const QUOTES: [char; 2] = ['"', '\''];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub source: SkillSource,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkillSource {
    Claude,
    Shared(PathBuf),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SkillFile {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) file: PathBuf,
}

struct Found {
    skill: SkillFile,
    shared: bool,
}

fn found_skills(host: &HostLink, home: Option<&Path>, project: Option<&Path>) -> Vec<Found> {
    let bases: Vec<&Path> = project.into_iter().chain(home).collect();
    let linked: HashSet<OsString> = bases
        .iter()
        .flat_map(|base| entry_names(host, &base.join(CLAUDE_DIR).join(SKILLS_DIR)))
        .collect();
    let mut found = Vec::new();
    for base in bases {
        found.extend(skills_in(host, &base.join(CLAUDE_DIR), None));
        found.extend(shared_skills(host, &base.join(AGENTS_DIR), &linked));
    }
    for (plugin, root) in home.map_or_default(|home| enabled_plugins(host, home)) {
        found.extend(skills_in(host, &root, Some(&plugin)));
    }
    let mut seen = HashSet::new();
    found.retain(|found| seen.insert(found.skill.name.clone()));
    found.sort_by_cached_key(|found| found.skill.name.to_lowercase());
    found
}

#[must_use]
pub fn claude_skills(host: &HostLink, home: Option<&Path>, project: Option<&Path>) -> Vec<Skill> {
    found_skills(host, home, project)
        .into_iter()
        .map(|Found { skill, shared }| Skill {
            name: skill.name,
            description: skill.description,
            source: if shared {
                SkillSource::Shared(skill.file)
            } else {
                SkillSource::Claude
            },
        })
        .collect()
}

pub(crate) fn skill_files(
    host: &HostLink,
    home: Option<&Path>,
    project: Option<&Path>,
) -> Vec<SkillFile> {
    found_skills(host, home, project)
        .into_iter()
        .map(|found| found.skill)
        .collect()
}

#[must_use]
pub fn search_skills<'a>(skills: &'a [Skill], query: &str, limit: usize) -> Vec<&'a Skill> {
    let query = query.to_lowercase();
    let mut found: Vec<(bool, &Skill)> = skills
        .iter()
        .filter_map(|skill| {
            let name = skill.name.to_lowercase();
            name.contains(&query)
                .then(|| (!name.starts_with(&query), skill))
        })
        .collect();
    found.sort_by_key(|(later, _)| *later);
    found
        .into_iter()
        .take(limit)
        .map(|(_, skill)| skill)
        .collect()
}

fn skills_in(host: &HostLink, root: &Path, plugin: Option<&str>) -> Vec<Found> {
    let named = |name: String| match plugin {
        Some(plugin) => format!("{plugin}{NAMESPACE_SEPARATOR}{name}"),
        None => name,
    };
    let mut found: Vec<Found> = documents_in(host, root)
        .into_iter()
        .map(|(name, description, file)| Found {
            skill: SkillFile {
                name: named(name),
                description,
                file,
            },
            shared: false,
        })
        .collect();
    let mut commands = Vec::new();
    command_files(host, &root.join(COMMANDS_DIR), &[], &mut commands);
    found.extend(commands.into_iter().filter_map(|(path, segments)| {
        let text = host.read_to_string(&path).ok()?;
        Some(Found {
            skill: SkillFile {
                name: named(segments.join(NAMESPACE_SEPARATOR)),
                description: FrontMatter::parse(&text)
                    .description
                    .unwrap_or_else(|| first_line(&text)),
                file: path,
            },
            shared: false,
        })
    }));
    found
}

fn shared_skills(host: &HostLink, root: &Path, linked: &HashSet<OsString>) -> Vec<Found> {
    documents_in(host, root)
        .into_iter()
        .map(|(name, description, file)| {
            let folder = file.parent().and_then(Path::file_name);
            let shared = !folder.is_some_and(|folder| linked.contains(folder));
            Found {
                skill: SkillFile {
                    name,
                    description,
                    file,
                },
                shared,
            }
        })
        .collect()
}

fn entry_names(host: &HostLink, dir: &Path) -> Vec<OsString> {
    host.read_dir(dir).map_or_else(
        |_| Vec::new(),
        |entries| {
            entries
                .into_iter()
                .map(|entry| OsString::from(entry.name))
                .collect()
        },
    )
}

fn documents_in(host: &HostLink, root: &Path) -> Vec<(String, String, PathBuf)> {
    subfolders(host, &root.join(SKILLS_DIR))
        .into_iter()
        .filter_map(|folder| {
            let file = folder.join(SKILL_FILE);
            let front = FrontMatter::parse(&host.read_to_string(&file).ok()?);
            let name = front
                .name
                .or_else(|| Some(folder.file_name()?.to_string_lossy().into_owned()))?;
            Some((name, front.description.unwrap_or_default(), file))
        })
        .collect()
}

fn subfolders(host: &HostLink, dir: &Path) -> Vec<PathBuf> {
    host.read_dir(dir).map_or_else(
        |_| Vec::new(),
        |entries| {
            entries
                .into_iter()
                .filter(|entry| entry.metadata.is_dir())
                .map(|entry| entry.path)
                .collect()
        },
    )
}

fn command_files(
    host: &HostLink,
    dir: &Path,
    prefix: &[String],
    found: &mut Vec<(PathBuf, Vec<String>)>,
) {
    let Ok(entries) = host.read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry.path;
        let mut segments = prefix.to_vec();
        if entry.metadata.is_dir() {
            let Some(name) = path.file_name() else {
                continue;
            };
            segments.push(name.to_string_lossy().into_owned());
            command_files(host, &path, &segments, found);
        } else if path
            .extension()
            .is_some_and(|extension| extension == COMMAND_EXTENSION)
        {
            let Some(stem) = path.file_stem() else {
                continue;
            };
            segments.push(stem.to_string_lossy().into_owned());
            found.push((path, segments));
        }
    }
}

fn enabled_plugins(host: &HostLink, home: &Path) -> Vec<(String, PathBuf)> {
    let claude = home.join(CLAUDE_DIR);
    let read = |path: PathBuf| -> Option<Value> {
        serde_json::from_str(&host.read_to_string(&path).ok()?).ok()
    };
    let Some(settings) = read(claude.join(SETTINGS_FILE)) else {
        return Vec::new();
    };
    let Some(installed) = read(claude.join(PLUGINS_DIR).join(INSTALLED_PLUGINS_FILE)) else {
        return Vec::new();
    };
    let Some(enabled) = settings.get(ENABLED_PLUGINS_KEY).and_then(Value::as_object) else {
        return Vec::new();
    };
    enabled
        .iter()
        .filter(|(_, on)| on.as_bool() == Some(true))
        .filter_map(|(key, _)| {
            let root = installed
                .get(PLUGINS_KEY)?
                .get(key)?
                .as_array()?
                .first()?
                .get(INSTALL_PATH_KEY)?
                .as_str()?;
            let name = key.split(MARKETPLACE_SEPARATOR).next()?;
            Some((name.to_owned(), install_folder(home, root)))
        })
        .collect()
}

fn install_folder(home: &Path, printed: &str) -> PathBuf {
    match parse_wsl_path(home) {
        Some(WslPath { distro, .. }) if printed.starts_with(LINUX_ROOT) => {
            wsl_unc_path(&distro, printed)
        }
        _ => PathBuf::from(printed),
    }
}

fn first_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
        .to_owned()
}

#[derive(Debug, Default, PartialEq, Eq)]
struct FrontMatter {
    name: Option<String>,
    description: Option<String>,
}

impl FrontMatter {
    fn parse(text: &str) -> Self {
        let mut lines = text.lines();
        if lines.next().map(str::trim_end) != Some(FRONT_MATTER_FENCE) {
            return Self::default();
        }
        let block: Vec<&str> = lines
            .take_while(|line| line.trim_end() != FRONT_MATTER_FENCE)
            .collect();
        Self {
            name: scalar(&block, NAME_KEY),
            description: scalar(&block, DESCRIPTION_KEY),
        }
    }
}

fn scalar(block: &[&str], key: &str) -> Option<String> {
    let at = block.iter().position(|line| {
        line.strip_prefix(key)
            .is_some_and(|rest| rest.starts_with(':'))
    })?;
    let value = block[at][key.len() + 1..].trim();
    let value = if BLOCK_SCALARS.contains(&value) {
        block[at + 1..]
            .iter()
            .take_while(|line| line.is_empty() || line.starts_with(char::is_whitespace))
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        value.trim_matches(QUOTES).to_owned()
    };
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests;
