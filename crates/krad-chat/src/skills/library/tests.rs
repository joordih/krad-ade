use super::*;
use std::error::Error;
use std::fs;

fn write(path: &Path, text: &str) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}

fn temp_dir(name: &str) -> Result<PathBuf, Box<dyn Error>> {
    let dir = std::env::temp_dir().join(format!("krad-library-{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn place(harness: SkillHarness, scope: SkillScope, file: &str) -> SkillPlace {
    SkillPlace {
        harness,
        scope,
        plugin: None,
        file: PathBuf::from(file),
    }
}

fn document(kind: SkillKind, name: &str, text: &str, at: SkillPlace) -> Document {
    Document {
        key: SkillKey::new(kind, name),
        description: text.to_owned(),
        place: at,
        fingerprint: fingerprint(text),
    }
}

#[test]
fn the_same_skill_in_several_places_shows_once_with_every_place() {
    let skills = merged(vec![
        document(
            SkillKind::Skill,
            "lint",
            "same",
            place(
                SkillHarness::Shared,
                SkillScope::Global,
                "/h/.agents/skills/lint",
            ),
        ),
        document(
            SkillKind::Skill,
            "lint",
            "same",
            place(
                SkillHarness::ClaudeCode,
                SkillScope::Project,
                "/p/.claude/skills/lint",
            ),
        ),
    ]);
    assert_eq!(skills.len(), 1);
    assert!(!skills[0].clash);
    assert_eq!(
        skills[0].harnesses(),
        [SkillHarness::ClaudeCode, SkillHarness::Shared]
    );
    assert_eq!(skills[0].places[0].scope, SkillScope::Project);
    assert_eq!(
        skills[0].scopes(),
        [SkillScope::Project, SkillScope::Global]
    );
}

#[test]
fn a_name_with_different_content_shows_as_marked_separate_entries() {
    let skills = merged(vec![
        document(
            SkillKind::Skill,
            "deploy",
            "one",
            place(SkillHarness::ClaudeCode, SkillScope::Global, "/a"),
        ),
        document(
            SkillKind::Skill,
            "deploy",
            "two",
            place(SkillHarness::Codex, SkillScope::Global, "/b"),
        ),
        document(
            SkillKind::Agent,
            "deploy",
            "one",
            place(SkillHarness::Opencode, SkillScope::Global, "/c"),
        ),
    ]);
    let rows: Vec<(SkillKind, bool)> = skills
        .iter()
        .map(|skill| (skill.key.kind, skill.clash))
        .collect();
    assert_eq!(
        rows,
        [
            (SkillKind::Skill, true),
            (SkillKind::Skill, true),
            (SkillKind::Agent, false),
        ]
    );
}

#[test]
fn the_filter_matches_query_kind_and_harness() {
    let skill = LibrarySkill {
        key: SkillKey::new(SkillKind::Agent, "Reviewer"),
        description: "Reads the diff".to_owned(),
        places: vec![place(SkillHarness::Opencode, SkillScope::Project, "/r")],
        clash: false,
    };
    let filter = |query: &str, kind, harness| LibraryFilter {
        query: query.to_owned(),
        kind,
        harness,
    };
    assert!(skill.matches(&filter("review", None, None)));
    assert!(skill.matches(&filter("DIFF", Some(SkillKind::Agent), None)));
    assert!(!skill.matches(&filter("", Some(SkillKind::Skill), None)));
    assert!(skill.matches(&filter("", None, Some(SkillHarness::Opencode))));
    assert!(!skill.matches(&filter("", None, Some(SkillHarness::Codex))));
}

#[test]
fn skills_agents_and_commands_come_from_all_four_ecosystems() -> Result<(), Box<dyn Error>> {
    let home = temp_dir("home")?;
    let project = temp_dir("project")?;
    let shared = "---\nname: lint\ndescription: Lints the code\n---\nRun the linter.";
    write(&home.join(".agents/skills/lint/SKILL.md"), shared)?;
    write(&home.join(".claude/skills/lint/SKILL.md"), shared)?;
    write(&home.join(".codex/skills/lint/SKILL.md"), shared)?;
    write(
        &project.join(".claude/agents/reviewer.md"),
        "---\nname: reviewer\ndescription: Reviews changes\n---\nBe strict.",
    )?;
    write(
        &home.join(".codex/prompts/ship.md"),
        "---\ndescription: Ships a release\n---\nShip it.",
    )?;
    write(
        &project.join(".opencode/agent/docs.md"),
        "---\ndescription: Writes docs\nmode: subagent\n---\nWrite.",
    )?;
    write(
        &home.join(".config/opencode/command/test.md"),
        "Run the tests\n",
    )?;
    write(
        &project.join("opencode.json"),
        r#"{"command":{"fmt":{"template":"format","description":"Formats"}}}"#,
    )?;
    write(
        &home.join(".config/opencode/opencode.jsonc"),
        "{\n  // agents\n  \"agent\": { \"triage\": { \"description\": \"Sorts issues\", }, },\n}",
    )?;
    let skills = skill_library(&HostLink::Local, Some(&home), Some(&project));
    let rows: Vec<(&str, SkillKind, &str)> = skills
        .iter()
        .map(|skill| {
            (
                skill.key.name.as_str(),
                skill.key.kind,
                skill.description.as_str(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("docs", SkillKind::Agent, "Writes docs"),
            ("fmt", SkillKind::Command, "Formats"),
            ("lint", SkillKind::Skill, "Lints the code"),
            ("prompts:ship", SkillKind::Command, "Ships a release"),
            ("reviewer", SkillKind::Agent, "Reviews changes"),
            ("test", SkillKind::Command, "Run the tests"),
            ("triage", SkillKind::Agent, "Sorts issues"),
        ]
    );
    assert_eq!(
        skills[2].harnesses(),
        [
            SkillHarness::ClaudeCode,
            SkillHarness::Codex,
            SkillHarness::Shared
        ]
    );
    assert_eq!(skills[4].scopes(), [SkillScope::Project]);
    fs::remove_dir_all(home)?;
    fs::remove_dir_all(project)?;
    Ok(())
}

#[test]
fn enabled_count_leaves_out_disabled_keys() {
    let skill = |name: &str| LibrarySkill {
        key: SkillKey::new(SkillKind::Skill, name),
        description: String::new(),
        places: Vec::new(),
        clash: false,
    };
    let mut settings = SkillSettings::default();
    settings.set_enabled(SkillKey::new(SkillKind::Skill, "a"), false);
    assert_eq!(enabled_count(&[skill("a"), skill("b")], &settings), 1);
}
