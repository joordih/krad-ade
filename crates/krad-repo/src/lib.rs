mod atomic;
mod branch;
mod changes;
mod error;
mod file_ops;
mod files;
mod git;
mod index;
mod notes;
mod process;
mod reveal;
mod search;
mod status;
mod worktree;

pub use branch::{diff_between, remote_url, resolved_remote_url, ORIGIN_REMOTE};
pub use changes::{line_counts, working_diff, LineCount};
pub use error::RepoError;
pub use file_ops::{create_dir, create_file, delete_entry, move_entry, rename_entry};
pub use files::{list_dir, read_text, DirEntry, EntryKind, TextFile};
pub use git::repo_root;
pub use index::{index_files, FileIndex, IndexEntry};
pub use notes::{
    add_attachment, create_note, create_vault_folder, docs_source_candidates, read_docs_sources,
    read_note, save_attachment, stamp_new_notes, vault_dir, vault_entries, vault_notes, vault_path,
    write_docs_sources, write_note, write_note_text, SourceEntry, VaultEntry, VaultEntryKind,
};
pub use process::{wsl_home, wsl_programs};
pub use reveal::reveal_in_file_manager;
pub use search::{search_text, TextMatch};
pub use status::{parse_status, status, ChangeKind, FileChange, GitStatus};
pub use worktree::{
    add_worktree, remove_worktree, sanitize_worktree_name, worktree_path, Worktree,
};
