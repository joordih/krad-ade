use gpui_kit::component::diff::{DiffLinePosition, DiffSide};
use gpui_kit::SharedString;
use krad_domain::review::{Discussion, NotePosition};

use super::ChangedFile;

#[derive(Clone, PartialEq, Eq, Debug)]
pub(in crate::environment_shell) struct ThreadAnchor {
    pub(in crate::environment_shell) discussion: usize,
    pub(in crate::environment_shell) position: DiffLinePosition,
}

fn note_line(path: &SharedString, position: &NotePosition) -> Option<DiffLinePosition> {
    let (side, line) = match (position.new_line, position.old_line) {
        (Some(new), _) => (DiffSide::Modified, new),
        (None, Some(old)) => (DiffSide::Original, old),
        (None, None) => return None,
    };
    Some(DiffLinePosition::new(
        path.clone(),
        side,
        usize::try_from(line).ok()?,
    ))
}

pub(in crate::environment_shell) fn thread_anchors(
    files: &[ChangedFile],
    discussions: &[Discussion],
    shown: usize,
    path: &SharedString,
) -> Vec<ThreadAnchor> {
    discussions
        .iter()
        .enumerate()
        .filter_map(|(discussion, thread)| {
            let position = thread.notes.first()?.position.as_ref()?;
            let holder = files.iter().position(|file| file.holds(position))?;
            (holder == shown).then_some(ThreadAnchor {
                discussion,
                position: note_line(path, position)?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::review::{Change, DiscussionId, Note, NoteId, User, UserId};

    const DIFF: &str = "@@ -1,2 +1,3 @@\n a\n-b\n+c\n+d\n";

    fn file(path: &str) -> ChangedFile {
        ChangedFile::new(Change {
            old_path: path.to_owned(),
            new_path: path.to_owned(),
            diff: DIFF.to_owned(),
            new_file: false,
            renamed_file: false,
            deleted_file: false,
        })
    }

    fn thread(path: &str, new_line: Option<u32>, old_line: Option<u32>) -> Discussion {
        Discussion {
            id: DiscussionId(format!("{path}{new_line:?}{old_line:?}")),
            individual_note: false,
            notes: vec![Note {
                id: NoteId(1),
                body: String::new(),
                author: User {
                    id: UserId(1),
                    username: "ana".to_owned(),
                    name: "Ana".to_owned(),
                },
                created_at: String::new(),
                system: false,
                resolvable: true,
                resolved: false,
                position: Some(NotePosition {
                    old_path: Some(path.to_owned()),
                    new_path: Some(path.to_owned()),
                    old_line,
                    new_line,
                }),
            }],
        }
    }

    #[test]
    fn threads_hang_under_their_new_line_or_their_removed_old_line_of_the_shown_file() {
        let files = [file("a.rs"), file("b.rs")];
        let discussions = [
            thread("b.rs", Some(3), None),
            thread("a.rs", None, Some(2)),
            thread("c.rs", Some(1), None),
            thread("b.rs", Some(1), Some(1)),
        ];
        let path = SharedString::from("b.rs");
        let anchors = thread_anchors(&files, &discussions, 1, &path);
        assert_eq!(
            anchors,
            [
                ThreadAnchor {
                    discussion: 0,
                    position: DiffLinePosition::new("b.rs", DiffSide::Modified, 3),
                },
                ThreadAnchor {
                    discussion: 3,
                    position: DiffLinePosition::new("b.rs", DiffSide::Modified, 1),
                },
            ],
            "threads of other files are left out"
        );
        let first = thread_anchors(&files, &discussions, 0, &SharedString::from("a.rs"));
        assert_eq!(
            first,
            [ThreadAnchor {
                discussion: 1,
                position: DiffLinePosition::new("a.rs", DiffSide::Original, 2),
            }]
        );
    }
}
