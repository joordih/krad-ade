use krad_domain::{SkillKey, SkillKind};
use krad_host::HostLink;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

use super::super::{
    command_files, enabled_plugins, first_line, subfolders, FrontMatter, AGENTS_DIR, CLAUDE_DIR,
    COMMANDS_DIR, FRONT_MATTER_FENCE, NAMESPACE_SEPARATOR, SKILLS_DIR, SKILL_FILE,
};
use super::jsonc::plain_json;
use super::{fingerprint, Document, SkillHarness, SkillPlace, SkillScope};

const CLAUDE_AGENTS_DIR: &str = "agents";
const CODEX_DIR: &str = ".codex";
const CODEX_PROMPTS_DIR: &str = "prompts";
const CONFIG_DIR: &str = ".config";
const OPENCODE_DIR: &str = "opencode";
const OPENCODE_PROJECT_DIR: &str = ".opencode";
const OPENCODE_AGENT_DIRS: [&str; 2] = ["agent", "agents"];
const OPENCODE_COMMAND_DIRS: [&str; 2] = ["command", "commands"];
const OPENCODE_SKILL_DIRS: [&str; 2] = ["skill", "skills"];
const OPENCODE_CONFIG_FILES: [&str; 2] = ["opencode.json", "opencode.jsonc"];
const OPENCODE_AGENT_KEY: &str = "agent";
const OPENCODE_COMMAND_KEY: &str = "command";
const OPENCODE_DESCRIPTION_KEY: &str = "description";

#[derive(Clone, Copy)]
enum Layout {
    SkillFolders,
    Markdown,
    NestedMarkdown,
}

struct Location {
    harness: SkillHarness,
    scope: SkillScope,
    plugin: Option<String>,
    namespace: Option<String>,
    dir: PathBuf,
    layout: Layout,
    kind: SkillKind,
}

impl Location {
    fn place(&self, file: PathBuf) -> SkillPlace {
        SkillPlace {
            harness: self.harness,
            scope: self.scope,
            plugin: self.plugin.clone(),
            file,
        }
    }

    fn named(&self, name: String) -> String {
        match &self.namespace {
            Some(namespace) => format!("{namespace}{NAMESPACE_SEPARATOR}{name}"),
            None => name,
        }
    }
}

struct Base<'a> {
    root: &'a Path,
    scope: SkillScope,
}

fn locations(home: Option<&Path>, project: Option<&Path>) -> Vec<Location> {
    let bases: Vec<Base> = project
        .map(|root| Base {
            root,
            scope: SkillScope::Project,
        })
        .into_iter()
        .chain(home.map(|root| Base {
            root,
            scope: SkillScope::Global,
        }))
        .collect();
    let mut found = Vec::new();
    for base in &bases {
        let at = |harness, dir: PathBuf, layout, kind| Location {
            harness,
            scope: base.scope,
            plugin: None,
            namespace: None,
            dir,
            layout,
            kind,
        };
        let claude = base.root.join(CLAUDE_DIR);
        found.push(at(
            SkillHarness::ClaudeCode,
            claude.join(SKILLS_DIR),
            Layout::SkillFolders,
            SkillKind::Skill,
        ));
        found.push(at(
            SkillHarness::ClaudeCode,
            claude.join(CLAUDE_AGENTS_DIR),
            Layout::Markdown,
            SkillKind::Agent,
        ));
        found.push(at(
            SkillHarness::ClaudeCode,
            claude.join(COMMANDS_DIR),
            Layout::NestedMarkdown,
            SkillKind::Command,
        ));
        let codex = base.root.join(CODEX_DIR);
        found.push(at(
            SkillHarness::Codex,
            codex.join(SKILLS_DIR),
            Layout::SkillFolders,
            SkillKind::Skill,
        ));
        if base.scope == SkillScope::Global {
            found.push(Location {
                namespace: Some(CODEX_PROMPTS_DIR.to_owned()),
                ..at(
                    SkillHarness::Codex,
                    codex.join(CODEX_PROMPTS_DIR),
                    Layout::Markdown,
                    SkillKind::Command,
                )
            });
        }
        let opencode = opencode_dir(base);
        for dir in OPENCODE_SKILL_DIRS {
            found.push(at(
                SkillHarness::Opencode,
                opencode.join(dir),
                Layout::SkillFolders,
                SkillKind::Skill,
            ));
        }
        for dir in OPENCODE_AGENT_DIRS {
            found.push(at(
                SkillHarness::Opencode,
                opencode.join(dir),
                Layout::NestedMarkdown,
                SkillKind::Agent,
            ));
        }
        for dir in OPENCODE_COMMAND_DIRS {
            found.push(at(
                SkillHarness::Opencode,
                opencode.join(dir),
                Layout::NestedMarkdown,
                SkillKind::Command,
            ));
        }
        found.push(at(
            SkillHarness::Shared,
            base.root.join(AGENTS_DIR).join(SKILLS_DIR),
            Layout::SkillFolders,
            SkillKind::Skill,
        ));
    }
    found
}

fn opencode_dir(base: &Base) -> PathBuf {
    match base.scope {
        SkillScope::Project => base.root.join(OPENCODE_PROJECT_DIR),
        SkillScope::Global => base.root.join(CONFIG_DIR).join(OPENCODE_DIR),
    }
}

fn opencode_configs(home: Option<&Path>, project: Option<&Path>) -> Vec<(SkillScope, PathBuf)> {
    let folders = project
        .map(|root| (SkillScope::Project, root.to_path_buf()))
        .into_iter()
        .chain(home.map(|root| (SkillScope::Global, root.join(CONFIG_DIR).join(OPENCODE_DIR))));
    folders
        .flat_map(|(scope, folder)| OPENCODE_CONFIG_FILES.map(|name| (scope, folder.join(name))))
        .collect()
}

fn plugin_locations(host: &HostLink, home: Option<&Path>) -> Vec<Location> {
    home.map_or_default(|home| enabled_plugins(host, home))
        .into_iter()
        .flat_map(|(plugin, root)| {
            [
                (SKILLS_DIR, Layout::SkillFolders, SkillKind::Skill),
                (CLAUDE_AGENTS_DIR, Layout::Markdown, SkillKind::Agent),
                (COMMANDS_DIR, Layout::NestedMarkdown, SkillKind::Command),
            ]
            .map(|(dir, layout, kind)| Location {
                harness: SkillHarness::ClaudeCode,
                scope: SkillScope::Global,
                plugin: Some(plugin.clone()),
                namespace: Some(plugin.clone()),
                dir: root.join(dir),
                layout,
                kind,
            })
        })
        .collect()
}

pub(super) fn found_documents(
    host: &HostLink,
    home: Option<&Path>,
    project: Option<&Path>,
) -> Vec<Document> {
    let mut found: Vec<Document> = locations(home, project)
        .into_iter()
        .chain(plugin_locations(host, home))
        .flat_map(|location| documents_at(host, &location))
        .collect();
    for (scope, file) in opencode_configs(home, project) {
        found.extend(opencode_config_documents(host, scope, &file));
    }
    found
}

fn documents_at(host: &HostLink, location: &Location) -> Vec<Document> {
    match location.layout {
        Layout::SkillFolders => subfolders(host, &location.dir)
            .into_iter()
            .filter_map(|folder| {
                let file = folder.join(SKILL_FILE);
                let text = host.read_to_string(&file).ok()?;
                let stem = folder.file_name()?.to_string_lossy().into_owned();
                Some(markdown_document(location, file, &text, stem))
            })
            .collect(),
        Layout::Markdown | Layout::NestedMarkdown => {
            let mut files = Vec::new();
            command_files(host, &location.dir, &[], &mut files);
            files
                .into_iter()
                .filter(|(_, segments)| {
                    matches!(location.layout, Layout::NestedMarkdown) || segments.len() == 1
                })
                .filter_map(|(file, segments)| {
                    let text = host.read_to_string(&file).ok()?;
                    let stem = segments.join(NAMESPACE_SEPARATOR);
                    Some(markdown_document(location, file, &text, stem))
                })
                .collect()
        }
    }
}

fn markdown_document(location: &Location, file: PathBuf, text: &str, stem: String) -> Document {
    let front = FrontMatter::parse(text);
    let kind = location.kind;
    let name = match kind {
        SkillKind::Command => stem,
        SkillKind::Skill | SkillKind::Agent => front.name.unwrap_or(stem),
    };
    let description = front.description.unwrap_or_else(|| match kind {
        SkillKind::Command => first_line(body(text)),
        SkillKind::Skill | SkillKind::Agent => String::new(),
    });
    Document {
        key: SkillKey::new(kind, &location.named(name)),
        description,
        place: location.place(file),
        fingerprint: fingerprint(text),
    }
}

fn body(text: &str) -> &str {
    let Some(rest) = text.strip_prefix(FRONT_MATTER_FENCE) else {
        return text;
    };
    rest.split_once(FRONT_MATTER_FENCE)
        .map_or(text, |(_, body)| body)
}

fn opencode_config_documents(host: &HostLink, scope: SkillScope, file: &Path) -> Vec<Document> {
    let Some(config) = host
        .read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&plain_json(&text)).ok())
    else {
        return Vec::new();
    };
    [
        (OPENCODE_AGENT_KEY, SkillKind::Agent),
        (OPENCODE_COMMAND_KEY, SkillKind::Command),
    ]
    .into_iter()
    .filter_map(|(key, kind)| Some((config.get(key)?.as_object()?, kind)))
    .flat_map(|(entries, kind)| config_entries(entries, kind, scope, file))
    .collect()
}

fn config_entries(
    entries: &Map<String, Value>,
    kind: SkillKind,
    scope: SkillScope,
    file: &Path,
) -> Vec<Document> {
    entries
        .iter()
        .map(|(name, entry)| Document {
            key: SkillKey::new(kind, name),
            description: entry
                .get(OPENCODE_DESCRIPTION_KEY)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            place: SkillPlace {
                harness: SkillHarness::Opencode,
                scope,
                plugin: None,
                file: file.to_path_buf(),
            },
            fingerprint: fingerprint(&entry.to_string()),
        })
        .collect()
}
