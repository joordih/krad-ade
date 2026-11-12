use krad_domain::{
    AgentPreset, CloudPlacement, CloudProcessId, CloudRunnerId, NodeKind, Session, TerminalId,
    TerminalNode, DEFAULT_CHAT_HARNESS,
};
use std::path::Path;

const REPOS_DIR: &str = "repos";
const WORKTREES_DIR: &str = "worktrees";
const BRANCH_PREFIX: &str = "krad/";
const SLUG_SEPARATOR: char = '-';
const URL_SEPARATORS: [char; 2] = ['/', ':'];
const GIT_SUFFIX: &str = ".git";
const FALLBACK_SLUG: &str = "repo";
const NAME_SLUG_MAX: usize = 24;
const SESSION_SLUG_MAX: usize = 32;
const NONCE_LEN: usize = 8;
const PROCESS_NONCE_LEN: usize = 6;
const NONCE_FALLBACK: &str = "0";
const FNV_OFFSET: u32 = 0x811c_9dc5;
const FNV_PRIME: u32 = 0x0100_0193;
const SESSION_NAME_PREFIX: &str = "Cloud · ";
const REMOTE_BRANCH_SEPARATOR: char = '/';

fn fnv1a(text: &str) -> u32 {
    text.bytes().fold(FNV_OFFSET, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(FNV_PRIME)
    })
}

fn slug_of(raw: &str, limit: usize) -> String {
    let mut slug = String::with_capacity(raw.len().min(limit));
    for character in raw.chars().flat_map(char::to_lowercase) {
        if slug.len() >= limit {
            break;
        }
        if character.is_ascii_alphanumeric() {
            slug.push(character);
        } else if !slug.is_empty() && !slug.ends_with(SLUG_SEPARATOR) {
            slug.push(SLUG_SEPARATOR);
        }
    }
    let slug = slug.trim_end_matches(SLUG_SEPARATOR);
    if slug.is_empty() {
        FALLBACK_SLUG.to_owned()
    } else {
        slug.to_owned()
    }
}

pub(in crate::environment_shell) fn repo_slug(remote_url: &str) -> String {
    let trimmed = remote_url.trim().trim_end_matches('/');
    let last = trimmed
        .rsplit(URL_SEPARATORS)
        .next()
        .unwrap_or(trimmed)
        .trim_end_matches(GIT_SUFFIX);
    format!(
        "{}{SLUG_SEPARATOR}{:08x}",
        slug_of(last, NAME_SLUG_MAX),
        fnv1a(trimmed)
    )
}

pub(in crate::environment_shell) fn session_slug(group_name: &str, nonce: &str) -> String {
    format!(
        "{}{SLUG_SEPARATOR}{}",
        slug_of(group_name, NAME_SLUG_MAX),
        nonce_of(nonce, NONCE_LEN)
    )
}

pub(in crate::environment_shell) fn placement_for(
    runner: CloudRunnerId,
    runner_dir: &Path,
    repo_slug: &str,
    session_slug: &str,
) -> CloudPlacement {
    CloudPlacement {
        runner,
        repo_dir: runner_dir.join(REPOS_DIR).join(repo_slug),
        worktree: runner_dir
            .join(WORKTREES_DIR)
            .join(repo_slug)
            .join(session_slug),
        branch: format!("{BRANCH_PREFIX}{session_slug}"),
    }
}

pub(in crate::environment_shell) fn node_process_id(
    placement: &CloudPlacement,
    terminal: TerminalId,
    nonce: &str,
) -> Option<CloudProcessId> {
    let session = placement.worktree.file_name()?.to_string_lossy();
    let session = slug_of(&session, SESSION_SLUG_MAX);
    format!(
        "{session}{SLUG_SEPARATOR}{}{SLUG_SEPARATOR}{}",
        terminal.as_u64(),
        nonce_of(nonce, PROCESS_NONCE_LEN)
    )
    .parse()
    .ok()
}

pub(in crate::environment_shell) fn new_nonce() -> String {
    krad_host::new_install_id()
        .map_or_else(|| NONCE_FALLBACK.to_owned(), |id| id.as_str().to_owned())
}

fn nonce_of(raw: &str, length: usize) -> String {
    raw.chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .take(length)
        .collect()
}

pub(in crate::environment_shell) fn upstream_branch(upstream: Option<&str>) -> Option<&str> {
    let (_, branch) = upstream?.split_once(REMOTE_BRANCH_SEPARATOR)?;
    Some(branch).filter(|branch| !branch.is_empty())
}

pub(in crate::environment_shell) fn upstream_remote(upstream: Option<&str>) -> Option<&str> {
    let (remote, _) = upstream?.split_once(REMOTE_BRANCH_SEPARATOR)?;
    Some(remote).filter(|remote| !remote.is_empty())
}

pub(in crate::environment_shell) fn cloud_session_name(group: &str, ordinal: usize) -> String {
    if ordinal <= 1 {
        format!("{SESSION_NAME_PREFIX}{group}")
    } else {
        format!("{SESSION_NAME_PREFIX}{group} {ordinal}")
    }
}

pub(in crate::environment_shell) fn sent_session_name(session: &str) -> String {
    if session.starts_with(SESSION_NAME_PREFIX) {
        session.to_owned()
    } else {
        format!("{SESSION_NAME_PREFIX}{session}")
    }
}

pub(in crate::environment_shell) fn holds_cloud_work(
    session: &Session,
    nodes: &[TerminalNode],
) -> bool {
    session.cloud.is_some()
        && session
            .tabs
            .tabs
            .iter()
            .flat_map(|tab| &tab.tiles)
            .any(|tile| {
                nodes.iter().any(|node| {
                    node.id == tile.id && matches!(node.kind, NodeKind::Terminal | NodeKind::Chat)
                })
            })
}

pub(in crate::environment_shell) fn reached_limit(running: usize, max_sessions: u8) -> bool {
    running >= usize::from(max_sessions)
}

pub(in crate::environment_shell) fn offered_patch(diff: String) -> Option<String> {
    (!diff.trim().is_empty()).then_some(diff)
}

pub(in crate::environment_shell) fn cloud_chat_harness(
    preferred: AgentPreset,
    candidates: &[AgentPreset],
) -> AgentPreset {
    std::iter::once(preferred)
        .chain(candidates.iter().copied())
        .find(|preset| {
            preset.has_structured_chat()
                && *preset != AgentPreset::Shell
                && preset.unavailable_in_cloud().is_none()
        })
        .unwrap_or(DEFAULT_CHAT_HARNESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{built_in_acp_agents, AgentState, EnvironmentData, EnvironmentName};
    use std::path::PathBuf;

    #[test]
    fn a_remote_names_its_clone_by_repository_and_url() {
        let ssh = repo_slug("git@github.com:joordih/Krad-ADE.git");
        let https = repo_slug("https://github.com/joordih/krad-ade");
        assert!(ssh.starts_with("krad-ade-"));
        assert!(https.starts_with("krad-ade-"));
        assert_ne!(ssh, https, "two urls never share a clone");
        assert_eq!(ssh, repo_slug("git@github.com:joordih/Krad-ADE.git"));
        assert!(repo_slug("///").starts_with("repo-"));
    }

    #[test]
    fn a_cloud_session_lives_in_its_own_worktree_and_branch() {
        let slug = session_slug("Api Server!", "A1b2C3d4e5");
        assert_eq!(slug, "api-server-a1b2c3d4");
        let placement = placement_for(
            CloudRunnerId(2),
            Path::new("/home/me/.krad-ade/runner"),
            "api-0011aabb",
            &slug,
        );
        assert_eq!(
            placement.repo_dir,
            PathBuf::from("/home/me/.krad-ade/runner/repos/api-0011aabb")
        );
        assert_eq!(
            placement.worktree,
            PathBuf::from("/home/me/.krad-ade/runner/worktrees/api-0011aabb/api-server-a1b2c3d4")
        );
        assert_eq!(placement.branch, "krad/api-server-a1b2c3d4");
        assert_eq!(placement.runner, CloudRunnerId(2));
    }

    #[test]
    fn each_node_of_a_cloud_session_gets_its_own_process() -> Result<(), String> {
        let placement = placement_for(CloudRunnerId(1), Path::new("/r"), "api-1", "api-a1b2c3d4");
        let chat = node_process_id(&placement, TerminalId(7), "F00Dbeef").ok_or("no id")?;
        let shell = node_process_id(&placement, TerminalId(8), "f00dbeef").ok_or("no id")?;
        assert_eq!(chat.as_str(), "api-a1b2c3d4-7-f00dbe");
        assert_ne!(chat, shell);
        let again = node_process_id(&placement, TerminalId(7), &new_nonce()).ok_or("no id")?;
        assert_ne!(
            again, chat,
            "a restarted process never reuses an old journal"
        );
        let long = placement_for(CloudRunnerId(1), Path::new("/r"), "x", &"s".repeat(90));
        let longest = node_process_id(&long, TerminalId(u64::MAX), &new_nonce());
        assert!(longest.is_some());
        Ok(())
    }

    #[test]
    fn the_base_follows_the_upstream_branch_when_there_is_one() {
        assert_eq!(upstream_branch(Some("origin/feature/x")), Some("feature/x"));
        assert_eq!(upstream_remote(Some("fork/main")), Some("fork"));
        assert_eq!(upstream_branch(Some("main")), None);
        assert_eq!(upstream_branch(None), None);
        assert_eq!(upstream_remote(None), None);
    }

    #[test]
    fn cloud_sessions_are_named_after_their_group() {
        assert_eq!(cloud_session_name("api", 1), "Cloud · api");
        assert_eq!(cloud_session_name("api", 3), "Cloud · api 3");
        assert_eq!(sent_session_name("Fix login"), "Cloud · Fix login");
        assert_eq!(sent_session_name("Cloud · api"), "Cloud · api");
    }

    #[test]
    fn only_open_cloud_sessions_with_work_count_toward_the_limit() {
        let mut data = EnvironmentData::empty(EnvironmentName::new("main".to_owned()));
        let group = data.add_group(PathBuf::from("/work/api"), None);
        data.enter_sessions();
        let session = data.new_session(Some(group), krad_domain::SessionSpec::default());
        let placement = placement_for(CloudRunnerId(1), Path::new("/r"), "api-1", "api-a");
        let Some(found) = data.sessions.iter_mut().find(|found| found.id == session) else {
            unreachable!("the session was just made");
        };
        found.cloud = Some(placement);
        let count = |data: &EnvironmentData| {
            data.sessions
                .iter()
                .filter(|session| holds_cloud_work(session, &data.nodes))
                .count()
        };
        assert_eq!(count(&data), 0, "an empty cloud session runs nothing");
        let id = TerminalId(data.next_id);
        data.next_id += 1;
        data.nodes.push(TerminalNode {
            id,
            title: "Chat".to_owned(),
            state: AgentState::Idle,
            harness: Some(AgentPreset::ClaudeCode),
            resume: None,
            group: Some(group),
            text_scale: krad_domain::TextScale::default(),
            kind: NodeKind::Chat,
            file: None,
            extension: None,
            bookmarks: krad_domain::ChatBookmarks::default(),
            usage: krad_domain::ChatUsage::default(),
            checkpoints: krad_domain::ChatCheckpoints::default(),
            choice: krad_domain::ChatChoice::default(),
            cloud: None,
        });
        data.place_terminal(id, None);
        assert_eq!(count(&data), 1);
        assert!(reached_limit(4, 4));
        assert!(!reached_limit(3, 4));
        assert!(reached_limit(0, 0));
    }

    #[test]
    fn uncommitted_changes_are_offered_only_when_there_are_some() {
        assert_eq!(offered_patch(" \n".to_owned()), None);
        assert_eq!(
            offered_patch("diff --git a/x b/x\n".to_owned()).as_deref(),
            Some("diff --git a/x b/x\n")
        );
    }

    #[test]
    fn a_cloud_chat_never_starts_on_the_gateway() {
        let candidates = AgentPreset::chat_harnesses(&built_in_acp_agents());
        assert_eq!(
            cloud_chat_harness(AgentPreset::Gateway, &candidates),
            candidates
                .iter()
                .copied()
                .find(|preset| preset.unavailable_in_cloud().is_none()
                    && *preset != AgentPreset::Shell)
                .unwrap_or(DEFAULT_CHAT_HARNESS)
        );
        assert_eq!(
            cloud_chat_harness(AgentPreset::Codex, &candidates),
            AgentPreset::Codex
        );
        assert_eq!(
            cloud_chat_harness(AgentPreset::Gateway, &[]),
            DEFAULT_CHAT_HARNESS
        );
    }
}
