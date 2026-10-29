use krad_domain::CodeHost;

pub(in crate::environment_shell) struct HostWords {
    pub(super) requests: &'static str,
    pub(super) loading: &'static str,
    pub(super) empty: &'static str,
    pub(super) none_open: &'static str,
    pub(super) draft_placeholder: &'static str,
    pub(super) open_web: &'static str,
    pub(super) reload: &'static str,
    pub(super) diff_from: &'static str,
    pub(super) checking: &'static str,
    pub(super) blocks: &'static str,
    pub(super) no_pipeline: &'static str,
    pub(super) no_pipelines: &'static str,
    pub(super) sign_in: &'static str,
    pub(super) connecting: &'static str,
    pub(super) unavailable: &'static str,
}

const GITLAB: HostWords = HostWords {
    requests: "Merge requests",
    loading: "Loading merge requests…",
    empty: "No merge requests",
    none_open: "No merge request open",
    draft_placeholder: "Comment on this merge request…",
    open_web: "Open in GitLab",
    reload: "Reload from GitLab",
    diff_from: "from GitLab",
    checking: "GitLab is checking it",
    blocks: "GitLab blocks the merge",
    no_pipeline: "No pipeline ran for this merge request.",
    no_pipelines: "No pipelines ran for this merge request.",
    sign_in: "Sign in to GitLab",
    connecting: "Connecting to GitLab…",
    unavailable: "GitLab is unavailable",
};

const GITHUB: HostWords = HostWords {
    requests: "Pull requests",
    loading: "Loading pull requests…",
    empty: "No pull requests",
    none_open: "No pull request open",
    draft_placeholder: "Comment on this pull request…",
    open_web: "Open in GitHub",
    reload: "Reload from GitHub",
    diff_from: "from GitHub",
    checking: "GitHub is checking it",
    blocks: "GitHub blocks the merge",
    no_pipeline: "No Actions run for this pull request.",
    no_pipelines: "No Actions runs for this pull request.",
    sign_in: "Sign in to GitHub",
    connecting: "Connecting to GitHub…",
    unavailable: "GitHub is unavailable",
};

pub(in crate::environment_shell) const fn host_words(host: CodeHost) -> &'static HostWords {
    match host {
        CodeHost::Gitlab => &GITLAB,
        CodeHost::Github => &GITHUB,
    }
}
