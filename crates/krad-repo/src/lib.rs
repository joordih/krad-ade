mod atomic;
mod branch;
mod changes;
mod checkpoint;
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
mod patch;
mod process;
mod project_search;
mod refs;
mod reveal;
mod search;
mod session_changes;
mod status;
mod task_run;
mod tasks;
mod templates;
mod variant;
mod worktree;

pub use branch::{diff_between, remote_url, resolved_remote_url, ORIGIN_REMOTE};
pub use changes::{line_counts, working_diff, working_patch, LineCount};
pub use checkpoint::{
    checkpoint_changes, restore_checkpoint, snapshot_checkpoint, CheckpointChange,
    CheckpointChangeKind, RestoredCheckpoint,
};
pub use commit::{
    commit, pop_stash, stage, stage_all, stash_changes, unstage, unstage_all, CommitMode,
    CommitScope,
};
pub use discard::{discard_change, discard_folder, folder_changes, FolderChanges};
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
    add_attachment, add_note, create_note, create_vault_folder, docs_source_candidates,
    read_docs_sources, read_note, save_attachment, stamp_new_notes, vault_dir, vault_entries,
    vault_notes, vault_path, write_docs_sources, write_note, write_note_text, SourceEntry,
    VaultEntry, VaultEntryKind,
};
pub use patch::{
    parse_patch, revert_hunk, revert_hunks, FilePatch, Hunk, HunkLine, HunkLineKind, PatchBody,
    PatchStatus, StaleHunk,
};
pub use process::{host_programs, wsl_home, wsl_programs};
pub use project_search::{
    parse_globs, replace_in_file, replaced_line, search_project, undo_replace, CaseMatching,
    FileMatches, FoundLine, PatternSyntax, ProjectMatches, ProjectQuery, ReplaceUndo, ReplacedFile,
    Replacement, UndoOutcome, WordMatching,
};
pub use refs::{
    check_out, create_branch, create_tag, delete_tag, list_refs, Branch, BranchPlace, Refs,
};
pub use reveal::reveal_in_file_manager;
pub use search::{search_text, TextMatch};
pub use session_changes::{
    revert_session_file, revert_session_hunk, session_diff, snapshot_tree, SessionDiff,
};
pub use status::{parse_status, status, ChangeKind, FileChange, GitStatus};
pub use task_run::{run_task, TASK_RUN_TIMEOUT};
pub use tasks::{detect_tasks, TaskDetection};
pub use templates::{read_session_templates, templates_dir, StoredTemplate};
pub use variant::{
    apply_variant, discard_variant, variant_changes, variant_diff_summary, VariantChanges,
    VariantRemoval,
};
pub use worktree::{
    add_branch_worktree, add_worktree, list_worktrees, remove_settled_worktree, remove_worktree,
    sanitize_worktree_name, worktree_path, Worktree,
};
