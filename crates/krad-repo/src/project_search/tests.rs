use super::*;
use std::error::Error;
use std::fs;
use std::process::Command;

fn query(pattern: &str) -> ProjectQuery {
    ProjectQuery {
        pattern: pattern.to_owned(),
        ..ProjectQuery::default()
    }
}

fn scratch(label: &str) -> Result<PathBuf, Box<dyn Error>> {
    let dir = std::env::temp_dir().join(format!("krad-project-{label}-{}", std::process::id()));
    fs::create_dir_all(dir.join("src"))?;
    fs::create_dir_all(dir.join("target"))?;
    fs::write(dir.join("src/lib.rs"), "fn foo() {}\nlet food = Foo;\n")?;
    fs::write(dir.join("notes.md"), "foo and foo\n")?;
    fs::write(dir.join("target/out.rs"), "foo\n")?;
    Ok(dir)
}

fn places(found: &ProjectMatches, root: &Path) -> Vec<(String, u32, usize)> {
    found
        .files
        .iter()
        .flat_map(|file| {
            let path = file.path.strip_prefix(root).unwrap_or(&file.path);
            let path = path.to_string_lossy().into_owned();
            file.lines
                .iter()
                .map(move |line| (path.clone(), line.line, line.matches.len()))
        })
        .collect()
}

#[test]
fn globs_are_split_on_commas() {
    assert_eq!(parse_globs(" *.rs, src/** ,,"), ["*.rs", "src/**"]);
    assert_eq!(parse_globs("  "), Vec::<String>::new());
}

#[test]
fn the_query_options_become_git_grep_flags_and_pathspecs() {
    let regex = ProjectQuery {
        pattern: "-fo+".to_owned(),
        syntax: PatternSyntax::Regex,
        case: CaseMatching::Sensitive,
        word: WordMatching::WholeWord,
        include: vec!["*.rs".to_owned()],
        exclude: vec!["./target".to_owned()],
    };
    let args: Vec<String> = grep_args(&regex, true, true)
        .into_iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        args,
        [
            "grep",
            "--line-number",
            "-z",
            "-I",
            "--no-color",
            "--untracked",
            "--extended-regexp",
            "--word-regexp",
            "--only-matching",
            "--column",
            "-e",
            "-fo+",
            "--",
            ":(glob)**/*.rs",
            ":(glob)**/*.rs/**",
            ":(exclude,glob)**/target",
            ":(exclude,glob)**/target/**",
        ]
    );
    let literal = grep_args(&query("x"), false, false);
    assert!(literal.contains(&OsString::from("--no-index")));
    assert!(literal.contains(&OsString::from("--fixed-strings")));
    assert!(literal.contains(&OsString::from("--ignore-case")));
}

#[test]
fn a_plain_folder_is_searched_with_case_words_and_globs() -> Result<(), Box<dyn Error>> {
    let dir = scratch("plain")?;
    let any_case = search_project(&HostLink::Local, &dir, &query("foo"), 100);
    let exact = ProjectQuery {
        case: CaseMatching::Sensitive,
        word: WordMatching::WholeWord,
        exclude: vec!["target".to_owned()],
        ..query("foo")
    };
    let words = search_project(&HostLink::Local, &dir, &exact, 100);
    let rust_only = ProjectQuery {
        include: vec!["*.rs".to_owned()],
        exclude: vec!["target".to_owned()],
        syntax: PatternSyntax::Regex,
        ..query("fo+d?")
    };
    let rust = search_project(&HostLink::Local, &dir, &rust_only, 100);
    let invalid = ProjectQuery {
        syntax: PatternSyntax::Regex,
        ..query("(")
    };
    let failed = search_project(&HostLink::Local, &dir, &invalid, 100);
    fs::remove_dir_all(&dir)?;

    let mut any_case = places(&any_case?, &dir);
    any_case.sort();
    assert_eq!(
        any_case,
        [
            ("notes.md".to_owned(), 1, 2),
            ("src/lib.rs".to_owned(), 1, 1),
            ("src/lib.rs".to_owned(), 2, 2),
            ("target/out.rs".to_owned(), 1, 1),
        ]
    );
    let mut words = places(&words?, &dir);
    words.sort();
    assert_eq!(
        words,
        [
            ("notes.md".to_owned(), 1, 2),
            ("src/lib.rs".to_owned(), 1, 1),
        ]
    );
    let rust = rust?;
    assert_eq!(
        places(&rust, &dir),
        [
            ("src/lib.rs".to_owned(), 1, 1),
            ("src/lib.rs".to_owned(), 2, 2)
        ]
    );
    assert_eq!(rust.files[0].lines[1].matches, [4..8, 11..14]);
    assert!(failed.is_err());
    Ok(())
}

#[test]
fn a_repository_search_skips_ignored_files_and_replaces_through_the_results(
) -> Result<(), Box<dyn Error>> {
    let dir = scratch("repo")?;
    let status = Command::new("git")
        .args(["init", "-q"])
        .current_dir(&dir)
        .status()?;
    assert!(status.success());
    fs::write(dir.join(".gitignore"), "target/\n")?;
    let found = search_project(&HostLink::Local, &dir, &query("foo"), 100);
    let found = found?;
    let lib = found
        .files
        .iter()
        .find(|file| file.path.ends_with("src/lib.rs"))
        .cloned();
    let replaced = lib
        .as_ref()
        .map(|file| replace_in_file(&HostLink::Local, file, "bar"))
        .transpose();
    let written = fs::read_to_string(dir.join("src/lib.rs"));
    fs::remove_dir_all(&dir)?;

    assert!(found
        .files
        .iter()
        .all(|file| !file.path.starts_with(dir.join("target"))));
    assert_eq!(found.match_count(), 5);
    assert_eq!(replaced?.map(|file| file.outcome.replaced), Some(3));
    assert_eq!(written?, "fn bar() {}\nlet bard = bar;\n");
    Ok(())
}

#[test]
fn an_empty_pattern_finds_nothing() -> Result<(), RepoError> {
    assert_eq!(
        search_project(&HostLink::Local, Path::new("/"), &query(""), 10)?,
        ProjectMatches::default()
    );
    Ok(())
}
