use super::*;
use krad_extensions::api::manifest::{
    CommandContribution, Contributions, HostKindContribution, Program, TitlebarAction,
    TitlebarItemContribution, TitlebarSide, ViewContribution, ViewZone,
};
use krad_extensions::api::{ApiVersion, IdError};

fn restart_tool() -> Result<Vec<krad_extensions::api::manifest::ToolContribution>, IdError> {
    let folder = std::env::temp_dir().join(format!("krad-trust-sheet-{}", std::process::id()));
    let written = std::fs::create_dir_all(&folder).and_then(|()| {
        std::fs::write(
            folder.join("extension.toml"),
            "id = \"t\"\nname = \"T\"\nversion = \"1\"\napi = \"1.0\"\n\n[program]\nplacement = \"app\"\nruntime = \"node\"\n\n[[contributes.tool]]\nname = \"restart_container\"\ndescription = \"Restart\"\neffect = \"destructive\"\n",
        )
    });
    let tools = written
        .ok()
        .and_then(|()| {
            krad_extensions::program::read_installed(&folder)
                .ok()
                .flatten()
        })
        .map_or_else(Vec::new, |installed| installed.manifest.contributes.tools);
    if tools.is_empty() {
        return Err(IdError::Empty);
    }
    Ok(tools)
}

fn docker() -> Result<ExtensionManifest, IdError> {
    Ok(ExtensionManifest {
        id: "docker".parse()?,
        name: "Docker".to_owned(),
        version: "0.1.0".to_owned(),
        description: None,
        api: ApiVersion::new(1, 0),
        program: Some(Program {
            placement: Placement::App,
            permissions: vec![Permission::Terminals],
            launch: Launch::Runtime {
                runtime: "node".parse()?,
                args: vec!["main.js".to_owned()],
            },
        }),
        activation: Vec::new(),
        contributes: Contributions {
            commands: vec![CommandContribution {
                id: "refresh".parse()?,
                title: "Docker: Refresh".to_owned(),
                icon: None,
            }],
            views: vec![ViewContribution {
                id: "containers".parse()?,
                title: "Containers".to_owned(),
                zone: ViewZone::Sidebar,
                icon: None,
            }],
            tools: restart_tool()?,
            host_kinds: vec![HostKindContribution {
                id: "container".parse()?,
                title: "Container".to_owned(),
                icon: None,
            }],
            titlebar_items: vec![TitlebarItemContribution {
                id: "status".parse()?,
                title: "Docker status".to_owned(),
                icon: krad_extensions::api::Icon::Layers,
                side: TitlebarSide::Right,
                action: TitlebarAction::Command("refresh".parse()?),
            }],
            ..Contributions::default()
        },
    })
}

#[test]
fn the_trust_sheet_lists_where_it_runs_what_it_may_do_and_what_it_adds() -> Result<(), IdError> {
    let lines = declared_lines(Some(&docker()?), &ManifestSummary::default());
    assert_eq!(lines.first().map(String::as_str), Some(RUNS_APP));
    assert!(lines.iter().any(|line| line == "Starts with node main.js"));
    assert!(lines.iter().any(|line| line == TERMINALS_WARNING));
    assert!(lines.iter().any(|line| line == HOST_KINDS_WARNING));
    assert!(lines.iter().any(|line| line == "Commands: Docker: Refresh"));
    assert!(lines.iter().any(|line| line == "Views: Containers"));
    assert!(lines
        .iter()
        .any(|line| line == &format!("{TITLEBAR_ITEMS}: Docker status")));
    assert!(
        lines
            .iter()
            .any(|line| line == &format!("{TOOLS}: restart_container ({DECLARED_DESTRUCTIVE})")),
        "{lines:?}"
    );
    Ok(())
}

#[test]
fn a_manifest_only_extension_lists_the_runtimes_it_would_change() {
    let summary = ManifestSummary {
        server_runtimes: vec!["node".to_owned()],
        languages: vec!["zig".to_owned()],
        ..ManifestSummary::default()
    };
    let lines = declared_lines(None, &summary);
    assert!(lines
        .iter()
        .any(|line| line.starts_with(SERVER_RUNTIMES) && line.ends_with("node")));
    assert!(lines.iter().any(|line| line.ends_with("zig")));
}

#[test]
fn names_cannot_hide_lines_or_run_on_forever() {
    assert_eq!(plain("Docker\nTrusted by Krad"), "Docker Trusted by Krad");
    let long = "x".repeat(MAX_SHOWN_CHARS * 2);
    assert_eq!(plain(&long).chars().count(), MAX_SHOWN_CHARS + 1);
}

#[test]
fn trust_is_stored_only_for_the_extension_that_was_shown() -> Result<(), Box<dyn std::error::Error>>
{
    let entry = |id: &str| -> Result<krad_domain::ExtensionEntry, IdError> {
        Ok(krad_domain::ExtensionEntry {
            id: id.parse()?,
            source: krad_domain::ExtensionSource::Folder("/x".into()),
            enabled: true,
            trusted: None,
        })
    };
    let mut entries = vec![entry("docker")?, entry("other")?];
    let shown = ExtensionFingerprint("seen".to_owned());
    assert!(store_trust(&mut entries, &"docker".parse()?, &shown));
    assert_eq!(entries[0].trusted.as_ref(), Some(&shown));
    assert_eq!(entries[1].trusted, None);
    assert!(!store_trust(&mut entries, &"missing".parse()?, &shown));
    Ok(())
}
