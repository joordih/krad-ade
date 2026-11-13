use std::error::Error;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command as Process;

const GIT: &str = "git";
const IDENTITY: [&str; 4] = [
    "-c",
    "user.name=Krad Test",
    "-c",
    "user.email=krad@test.invalid",
];
const WORKTREES_SUFFIX: &str = ".worktrees";
const MAIN_BRANCH: &str = "main";
pub const TRACKED: &str = "notes.txt";
pub const FIRST_TEXT: &str = "first\n";

pub struct Repo(pub PathBuf);

impl Drop for Repo {
    fn drop(&mut self) {
        let mut worktrees = OsString::from(self.0.as_os_str());
        worktrees.push(WORKTREES_SUFFIX);
        drop(std::fs::remove_dir_all(PathBuf::from(worktrees)));
        drop(std::fs::remove_dir_all(&self.0));
    }
}

pub fn git(repo: &Path, args: &[&str]) -> Result<(), Box<dyn Error>> {
    let status = Process::new(GIT)
        .args(IDENTITY)
        .arg("-C")
        .arg(repo)
        .args(args)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("git {args:?} failed").into())
    }
}

pub fn committed_repo(name: &str) -> Result<Repo, Box<dyn Error>> {
    let root = std::env::temp_dir().join(format!("krad-runtime-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&root)?;
    let repo = Repo(root.canonicalize()?);
    std::fs::write(repo.0.join(TRACKED), FIRST_TEXT)?;
    git(
        &repo.0,
        &["init", "--quiet", "--initial-branch", MAIN_BRANCH],
    )?;
    git(&repo.0, &["add", "--all"])?;
    git(&repo.0, &["commit", "--quiet", "--message", "first"])?;
    Ok(repo)
}
