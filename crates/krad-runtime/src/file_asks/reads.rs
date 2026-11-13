use krad_host::HostLink;
use krad_protocol::{Answer, OverLimit, Unreadable, MAX_READ_BYTES};
use krad_repo::{read_text, RepoError};
use std::path::Path;

fn text(bytes: Vec<u8>) -> Result<String, Answer> {
    String::from_utf8(bytes).map_err(|_| Answer::Unreadable {
        unreadable: Unreadable::NotText,
    })
}

pub(super) fn read(
    link: &HostLink,
    path: &Path,
    (from, limit): (u64, u64),
    over: OverLimit,
) -> Result<Answer, RepoError> {
    let limit = limit.min(MAX_READ_BYTES);
    let metadata = link.metadata(path)?;
    let len = metadata.len;
    let rest = len.saturating_sub(from);
    let modified = metadata.modified;
    match over {
        OverLimit::Refuse if rest > limit => Ok(Answer::Unreadable {
            unreadable: Unreadable::TooLarge { len, limit },
        }),
        OverLimit::Refuse => Ok(text(link.read_range(path, from, limit)?).map_or_else(
            |unreadable| unreadable,
            |text| Answer::FileText {
                text,
                len,
                truncated: false,
                modified,
            },
        )),
        OverLimit::Truncate if from == 0 => match read_text(link, path, limit) {
            Ok(file) => Ok(Answer::FileText {
                text: file.text,
                len,
                truncated: file.truncated,
                modified,
            }),
            Err(RepoError::Binary) => Ok(Answer::Unreadable {
                unreadable: Unreadable::NotText,
            }),
            Err(error) => Err(error),
        },
        OverLimit::Truncate => {
            let bytes = link.read_range(path, from, limit)?;
            let read = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
            Ok(Answer::FileText {
                text: String::from_utf8_lossy(&bytes).into_owned(),
                len,
                truncated: from.saturating_add(read) < len,
                modified,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    use std::fs;

    #[test]
    fn a_read_in_chunks_says_how_much_is_left_and_a_refused_one_says_why(
    ) -> Result<(), Box<dyn Error>> {
        let dir = std::env::temp_dir().join(format!("krad-file-reads-{}", std::process::id()));
        fs::create_dir_all(&dir)?;
        let file = dir.join("notes.txt");
        fs::write(&file, "abcdef")?;
        let chunk = read(&HostLink::Local, &file, (2, 2), OverLimit::Truncate)?;
        assert!(matches!(
            chunk,
            Answer::FileText { ref text, len: 6, truncated: true, .. } if text == "cd"
        ));
        let refused = read(&HostLink::Local, &file, (0, 3), OverLimit::Refuse)?;
        assert_eq!(
            refused,
            Answer::Unreadable {
                unreadable: Unreadable::TooLarge { len: 6, limit: 3 }
            }
        );
        fs::write(&file, [0xff, 0xfe])?;
        let binary = read(&HostLink::Local, &file, (0, 3), OverLimit::Refuse)?;
        assert_eq!(
            binary,
            Answer::Unreadable {
                unreadable: Unreadable::NotText
            }
        );
        fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
