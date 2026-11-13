use krad_host::HostLink;
use krad_protocol::{Answer, ErrorCode, FileAsk, VaultNoteText, MAX_ATTACHMENT_BYTES};
use krad_repo::{
    create_note, create_vault_folder, save_attachment, stamp_new_notes, vault_entries, vault_notes,
    vault_path, write_note_text, RepoError,
};
use std::time::{SystemTime, UNIX_EPOCH};

use super::stat;

const TOO_LARGE_ATTACHMENT: &str = "the attachment is too large";

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

pub(super) fn answer(link: &HostLink, ask: FileAsk) -> Result<Answer, RepoError> {
    Ok(match ask {
        FileAsk::Vault { root } => {
            stamp_new_notes(link, &root, now_secs())?;
            Answer::Vault {
                entries: vault_entries(link, &root)?,
                notes: vault_notes(link, &root)?
                    .into_iter()
                    .map(|(path, note)| VaultNoteText {
                        path,
                        text: note.to_markdown(),
                    })
                    .collect(),
            }
        }
        FileAsk::CreateNote {
            root,
            folder,
            title,
        } => Answer::NoteAdded {
            name: create_note(link, &root, &folder, &title)?,
        },
        FileAsk::CreateVaultFolder { root, folder, name } => Answer::NoteAdded {
            name: create_vault_folder(link, &root, &folder, &name)?,
        },
        FileAsk::WriteNote { root, path, text } => {
            write_note_text(link, &root, &path, &text)?;
            Answer::Written {
                stat: match vault_path(&root, &path) {
                    Some(file) => stat(link, &file)?,
                    None => None,
                },
            }
        }
        FileAsk::SaveAttachment { bytes, .. } if bytes.len() > MAX_ATTACHMENT_BYTES => {
            Answer::Failed {
                code: ErrorCode::Invalid,
                message: TOO_LARGE_ATTACHMENT.to_owned(),
            }
        }
        FileAsk::SaveAttachment {
            root,
            stem,
            extension,
            bytes,
        } => Answer::NoteAdded {
            name: save_attachment(link, &root, &stem, &extension, &bytes)?,
        },
        _ => Answer::Unknown,
    })
}
