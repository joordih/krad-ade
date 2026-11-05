mod atomic;
mod branch;
mod changes;
mod commit;
mod discard;
mod error;
mod file_ops;
mod files;
mod git;
mod history;
mod index;
mod network;
mod notes;
mod on_host;
mod process;
mod refs;
mod reveal;
mod search;
mod status;
mod worktree;

pub use branch::{diff_between, remote_url, resolved_remote_url, ORIGIN_REMOTE};
pub use changes::{line_counts, working_diff, LineCount};
pub use commit::{
    commit, pop_stash, stage, stage_all, stash_changes, unstage, unstage_all, CommitMode,
    CommitScope,
};
pub use discard::discard_change;
pub use error::RepoError;
pub use file_ops::{
    create_dir, create_file, delete_entry, move_entry, relocate_entry, rename_entry,
};
pub use files::{list_dir, read_text, DirEntry, EntryKind, TextFile};
pub use git::repo_root;
pub use history::{commit_diff, commit_files, commit_log, CommitFile};
pub use index::{index_files, FileIndex, IndexEntry};
pub use network::{fetch, pull, push, push_tags, sync, Askpass, PushTarget};
pub use notes::{
    add_attachment, create_note, create_vault_folder, docs_source_candidates, read_docs_sources,
    read_note, save_attachment, stamp_new_notes, vault_dir, vault_entries, vault_notes, vault_path,
    write_docs_sources, write_note, write_note_text, SourceEntry, VaultEntry, VaultEntryKind,
};
pub use process::{host_programs, wsl_home, wsl_programs};
pub use refs::{
    check_out, create_branch, create_tag, delete_tag, list_refs, Branch, BranchPlace, Refs,
};
pub use reveal::reveal_in_file_manager;
pub use search::{search_text, TextMatch};
pub use status::{parse_status, status, ChangeKind, FileChange, GitStatus};
pub use worktree::{
    add_branch_worktree, add_worktree, list_worktrees, remove_worktree, sanitize_worktree_name,
    worktree_path, Worktree,
};
