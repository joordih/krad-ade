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
