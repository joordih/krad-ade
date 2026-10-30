use super::parse::BAD_ARGUMENTS_TEXT;
use super::*;
use serde_json::json;

fn workspace() -> Workspace {
    Workspace {
        cwd: PathBuf::from("/work/app"),
        env: Vec::new(),
    }
}

#[test]
fn tools_parse_their_arguments_and_explain_bad_ones() {
    let read = Action::parse(READ_TOOL, &json!({"path": "src/a.rs", "offset": 3}));
    assert_eq!(
        read,
        Ok(Action::Read {
            path: PathBuf::from("src/a.rs"),
            offset: Some(3),
            limit: None,
        })
    );
    let shell = Action::parse(SHELL_TOOL, &json!({"command": "ls", "timeout_seconds": 5}));
    assert_eq!(
        shell,
        Ok(Action::Shell {
            command: "ls".to_owned(),
            timeout: Some(Duration::from_secs(5)),
            run: ShellRun::Wait,
        })
    );
    let background = Action::parse(
        SHELL_TOOL,
        &json!({"command": "npm run dev", "run_in_background": true}),
    );
    assert!(matches!(
        background,
        Ok(Action::Shell {
            run: ShellRun::Background,
            ..
        })
    ));
    assert_eq!(
        Action::parse(SHELL_OUTPUT_TOOL, &json!({"id": "bg1"}))
            .map(|action| action.summary(&workspace())),
        Ok("bg1".to_owned())
    );
    let fetch = Action::parse(
        WEB_FETCH_TOOL,
        &json!({"url": "https://docs.rs/x", "prompt": " "}),
    );
    assert!(matches!(&fetch, Ok(Action::WebFetch { prompt: None, .. })));
    let asking = Action::parse(
        WEB_FETCH_TOOL,
        &json!({"url": "https://docs.rs/x", "prompt": "the latest version"}),
    );
    assert!(matches!(
        &asking,
        Ok(Action::WebFetch { prompt: Some(prompt), .. }) if prompt == "the latest version"
    ));
    assert_eq!(
        fetch.map(|action| action.label().into_owned()),
        Ok(WEB_FETCH_LABEL.to_owned())
    );
    let remote = Action::parse("mcp__docs__search", &json!({"q": "serde"}));
    assert_eq!(
        remote
            .as_ref()
            .map(|action| (action.label().into_owned(), action.summary(&workspace()))),
        Ok(("docs · search".to_owned(), "q: serde".to_owned()))
    );
    assert!(
        Action::parse(WEB_FETCH_TOOL, &json!({"url": "file:///etc/passwd"}))
            .is_err_and(|error| error.starts_with(BAD_ARGUMENTS_TEXT))
    );
    assert!(Action::parse(WEB_SEARCH_TOOL, &json!({"query": "  "})).is_err());
    let missing = Action::parse(EDIT_TOOL, &json!({"path": "a.rs"}));
    assert!(missing.is_err_and(|error| error.starts_with(BAD_ARGUMENTS_TEXT)));
    assert_eq!(
        Action::parse("teleport", &json!({})),
        Err("Unknown tool teleport".to_owned())
    );
}

#[test]
fn every_spec_names_a_tool_that_parses() {
    assert!(specs()
        .iter()
        .all(|spec| Action::parse(&spec.name, &json!({}))
            != Err(format!("Unknown tool {}", spec.name))));
}

#[test]
fn paths_show_relative_inside_the_workspace_and_absolute_outside() {
    let workspace = workspace();
    let inside = Action::Read {
        path: PathBuf::from("./src/../src/a.rs"),
        offset: None,
        limit: None,
    };
    assert_eq!(inside.summary(&workspace), "src/a.rs");
    assert!(workspace.contains(Path::new("src/a.rs")));
    assert!(!workspace.contains(Path::new("../other/a.rs")));
    let outside = Action::Edit {
        path: PathBuf::from("/etc/hosts"),
        replacements: Vec::new(),
    };
    assert_eq!(outside.summary(&workspace), "/etc/hosts");
    assert_eq!(outside.label(), EDIT_LABEL);
    let grep = Action::Grep {
        pattern: "todo".to_owned(),
        path: Some(PathBuf::from("src")),
        glob: None,
        ignore_case: false,
    };
    assert_eq!(grep.summary(&workspace), "todo in src");
    assert_eq!(
        grep.resolve(&workspace),
        Some(PathBuf::from("/work/app/src"))
    );
}
