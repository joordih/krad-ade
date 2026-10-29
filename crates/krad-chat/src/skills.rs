use krad_domain::{parse_wsl_path, wsl_unc_path, WslPath};
use serde_json::Value;
use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
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

#[must_use]
pub fn claude_skills(home: Option<&Path>, project: Option<&Path>) -> Vec<Skill> {
    let bases: Vec<&Path> = project.into_iter().chain(home).collect();
    let linked: HashSet<OsString> = bases
        .iter()
        .flat_map(|base| entry_names(&base.join(CLAUDE_DIR).join(SKILLS_DIR)))
        .collect();
    let mut skills = Vec::new();
    for base in bases {
        skills.extend(skills_in(&base.join(CLAUDE_DIR), None));
        skills.extend(shared_skills(&base.join(AGENTS_DIR), &linked));
    }
    for (plugin, root) in home.map_or_default(enabled_plugins) {
        skills.extend(skills_in(&root, Some(&plugin)));
    }
    let mut seen = HashSet::new();
    skills.retain(|skill| seen.insert(skill.name.clone()));
    skills.sort_by_cached_key(|skill| skill.name.to_lowercase());
    skills
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

fn skills_in(root: &Path, plugin: Option<&str>) -> Vec<Skill> {
    let named = |name: String| match plugin {
        Some(plugin) => format!("{plugin}{NAMESPACE_SEPARATOR}{name}"),
        None => name,
    };
    let mut skills: Vec<Skill> = skill_files(root)
        .into_iter()
        .map(|(name, description, _)| Skill {
            name: named(name),
            description,
            source: SkillSource::Claude,
        })
        .collect();
    let mut commands = Vec::new();
    command_files(&root.join(COMMANDS_DIR), &[], &mut commands);
    skills.extend(commands.into_iter().filter_map(|(path, segments)| {
        let text = fs::read_to_string(&path).ok()?;
        Some(Skill {
            name: named(segments.join(NAMESPACE_SEPARATOR)),
            description: FrontMatter::parse(&text)
                .description
                .unwrap_or_else(|| first_line(&text)),
            source: SkillSource::Claude,
        })
    }));
    skills
}

fn shared_skills(root: &Path, linked: &HashSet<OsString>) -> Vec<Skill> {
    skill_files(root)
        .into_iter()
        .map(|(name, description, file)| {
            let folder = file.parent().and_then(Path::file_name);
            let source = if folder.is_some_and(|folder| linked.contains(folder)) {
                SkillSource::Claude
            } else {
                SkillSource::Shared(file)
            };
            Skill {
                name,
                description,
                source,
            }
        })
        .collect()
}

fn entry_names(dir: &Path) -> Vec<OsString> {
    fs::read_dir(dir).map_or_else(
        |_| Vec::new(),
        |entries| entries.flatten().map(|entry| entry.file_name()).collect(),
    )
}

fn skill_files(root: &Path) -> Vec<(String, String, PathBuf)> {
    subfolders(&root.join(SKILLS_DIR))
        .into_iter()
        .filter_map(|folder| {
            let file = folder.join(SKILL_FILE);
            let front = FrontMatter::parse(&fs::read_to_string(&file).ok()?);
            let name = front
                .name
                .or_else(|| Some(folder.file_name()?.to_string_lossy().into_owned()))?;
            Some((name, front.description.unwrap_or_default(), file))
        })
        .collect()
}

fn subfolders(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir).map_or_else(
        |_| Vec::new(),
        |entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .collect()
        },
    )
}

fn command_files(dir: &Path, prefix: &[String], found: &mut Vec<(PathBuf, Vec<String>)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|entry| entry.path()) {
        let mut segments = prefix.to_vec();
        if path.is_dir() {
            let Some(name) = path.file_name() else {
                continue;
            };
            segments.push(name.to_string_lossy().into_owned());
            command_files(&path, &segments, found);
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

fn enabled_plugins(home: &Path) -> Vec<(String, PathBuf)> {
    let claude = home.join(CLAUDE_DIR);
    let read = |path: PathBuf| -> Option<Value> {
        serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
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
mod tests {
    use super::*;
    use std::error::Error;

    fn write(path: &Path, text: &str) -> Result<(), Box<dyn Error>> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, text)?;
        Ok(())
    }

    fn temp_dir(name: &str) -> Result<PathBuf, Box<dyn Error>> {
        let dir = std::env::temp_dir().join(format!("krad-chat-{name}-{}", std::process::id()));
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    #[test]
    fn front_matter_reads_plain_and_folded_scalars() {
        let text = "---\nname: \"review\"\ndescription: >\n  Reviews the diff\n  for bugs.\nother: x\n---\nBody";
        assert_eq!(
            FrontMatter::parse(text),
            FrontMatter {
                name: Some("review".to_owned()),
                description: Some("Reviews the diff for bugs.".to_owned()),
            }
        );
        assert_eq!(
            FrontMatter::parse("No front matter"),
            FrontMatter::default()
        );
    }

    #[test]
    fn skills_come_from_the_project_the_user_and_enabled_plugins() -> Result<(), Box<dyn Error>> {
        let home = temp_dir("home")?;
        let project = temp_dir("project")?;
        let plugin = home.join("plugin-install");
        write(
            &project.join(".claude/skills/deploy/SKILL.md"),
            "---\ndescription: Project deploy\n---\n",
        )?;
        write(
            &home.join(".claude/skills/deploy/SKILL.md"),
            "---\ndescription: User deploy\n---\n",
        )?;
        write(
            &home.join(".claude/commands/git/sync.md"),
            "Sync the branch\n",
        )?;
        write(
            &project.join(".agents/skills/lint/SKILL.md"),
            "---\ndescription: Shared lint\n---\n",
        )?;
        write(
            &plugin.join("skills/audit/SKILL.md"),
            "---\nname: audit\ndescription: Audit\n---\n",
        )?;
        write(&home.join("off-install/skills/hidden/SKILL.md"), "")?;
        write(
            &home.join(".claude/settings.json"),
            r#"{"enabledPlugins":{"tools@market":true,"off@market":false}}"#,
        )?;
        let installed = serde_json::json!({"plugins": {
            "tools@market": [{"installPath": plugin}],
            "off@market": [{"installPath": home.join("off-install")}],
        }});
        write(
            &home.join(".claude/plugins/installed_plugins.json"),
            &installed.to_string(),
        )?;
        let skills = claude_skills(Some(&home), Some(&project));
        let names: Vec<&str> = skills.iter().map(|skill| skill.name.as_str()).collect();
        assert_eq!(names, ["deploy", "git:sync", "lint", "tools:audit"]);
        assert_eq!(skills[0].description, "Project deploy");
        assert_eq!(skills[1].description, "Sync the branch");
        assert_eq!(
            skills[2].source,
            SkillSource::Shared(project.join(".agents/skills/lint/SKILL.md"))
        );
        assert_eq!(skills[0].source, SkillSource::Claude);
        fs::remove_dir_all(home)?;
        fs::remove_dir_all(project)?;
        Ok(())
    }

    #[test]
    fn a_shared_skill_claude_links_to_runs_as_claudes_own() -> Result<(), Box<dyn Error>> {
        let home = temp_dir("linked-home")?;
        write(
            &home.join(".agents/skills/grill/SKILL.md"),
            "---\nname: grill\n---\n",
        )?;
        write(
            &home.join(".agents/skills/lint/SKILL.md"),
            "---\nname: lint\n---\n",
        )?;
        write(&home.join(".claude/skills/grill"), "")?;
        let skills = claude_skills(Some(&home), None);
        let sources: Vec<(&str, &SkillSource)> = skills
            .iter()
            .map(|skill| (skill.name.as_str(), &skill.source))
            .collect();
        assert_eq!(
            sources,
            [
                ("grill", &SkillSource::Claude),
                (
                    "lint",
                    &SkillSource::Shared(home.join(".agents/skills/lint/SKILL.md"))
                ),
            ]
        );
        fs::remove_dir_all(home)?;
        Ok(())
    }

    #[test]
    fn plugins_of_a_claude_inside_wsl_are_reached_through_its_share() {
        let home = Path::new(r"\\wsl.localhost\Ubuntu\home\ana");
        let folder = install_folder(home, "/home/ana/.claude/plugins/cache/tools");
        assert_eq!(
            parse_wsl_path(&folder).map(|path| path.distro),
            Some("Ubuntu".to_owned())
        );
        assert_eq!(
            install_folder(Path::new("/home/ana"), "/home/ana/plugin"),
            PathBuf::from("/home/ana/plugin")
        );
    }

    #[test]
    fn search_puts_names_starting_with_the_query_first() {
        let skill = |name: &str| Skill {
            name: name.to_owned(),
            description: String::new(),
            source: SkillSource::Claude,
        };
        let skills = [skill("code-review"), skill("review"), skill("deploy")];
        let found: Vec<&str> = search_skills(&skills, "REV", 5)
            .into_iter()
            .map(|skill| skill.name.as_str())
            .collect();
        assert_eq!(found, ["review", "code-review"]);
    }
}
