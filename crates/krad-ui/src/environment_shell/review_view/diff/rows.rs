use krad_domain::review::Discussion;

use super::{anchor_line, ChangedFile};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::environment_shell) struct ThreadAnchor {
    pub(in crate::environment_shell) line: usize,
    pub(in crate::environment_shell) discussion: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::environment_shell) enum DiffItem {
    Line(usize),
    Thread(usize),
}

pub(in crate::environment_shell) fn thread_anchors(
    files: &[ChangedFile],
    discussions: &[Discussion],
) -> Vec<Vec<ThreadAnchor>> {
    let mut anchors: Vec<Vec<ThreadAnchor>> = vec![Vec::new(); files.len()];
    for (discussion, thread) in discussions.iter().enumerate() {
        let Some(position) = thread.notes.first().and_then(|note| note.position.as_ref()) else {
            continue;
        };
        let Some((file, lines)) = files
            .iter()
            .zip(anchors.iter_mut())
            .find(|(file, _)| file.holds(position))
        else {
            continue;
        };
        if let Some(line) = anchor_line(&file.lines, position) {
            lines.push(ThreadAnchor { line, discussion });
        }
    }
    for lines in &mut anchors {
        lines.sort_by_key(|anchor| (anchor.line, anchor.discussion));
    }
    anchors
}

pub(in crate::environment_shell) fn diff_items(
    line_count: usize,
    anchors: &[ThreadAnchor],
) -> Vec<DiffItem> {
    let mut items = Vec::with_capacity(line_count + anchors.len());
    let mut pending = anchors.iter().peekable();
    for line in 0..line_count {
        items.push(DiffItem::Line(line));
        while let Some(anchor) = pending.next_if(|anchor| anchor.line == line) {
            items.push(DiffItem::Thread(anchor.discussion));
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::review::{Change, DiscussionId, Note, NoteId, NotePosition, User, UserId};

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
    fn threads_are_anchored_once_to_their_file_and_line() {
        let files = [file("a.rs"), file("b.rs")];
        let discussions = [
            thread("b.rs", Some(3), None),
            thread("a.rs", None, Some(2)),
            thread("c.rs", Some(1), None),
            thread("b.rs", Some(1), None),
        ];
        let anchors = thread_anchors(&files, &discussions);
        assert_eq!(
            anchors[0],
            [ThreadAnchor {
                line: 2,
                discussion: 1
            }]
        );
        assert_eq!(
            anchors[1],
            [
                ThreadAnchor {
                    line: 1,
                    discussion: 3
                },
                ThreadAnchor {
                    line: 4,
                    discussion: 0
                }
            ],
            "sorted by line, the unknown file is skipped"
        );
    }

    #[test]
    fn every_line_is_one_item_with_its_threads_right_after_it() {
        let anchors = [
            ThreadAnchor {
                line: 1,
                discussion: 5,
            },
            ThreadAnchor {
                line: 1,
                discussion: 6,
            },
            ThreadAnchor {
                line: 3,
                discussion: 2,
            },
        ];
        assert_eq!(
            diff_items(4, &anchors),
            [
                DiffItem::Line(0),
                DiffItem::Line(1),
                DiffItem::Thread(5),
                DiffItem::Thread(6),
                DiffItem::Line(2),
                DiffItem::Line(3),
                DiffItem::Thread(2),
            ]
        );
        assert_eq!(diff_items(20_000, &[]).len(), 20_000, "nothing is cut");
    }
}
