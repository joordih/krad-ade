use krad_domain::JournalOffset;
use std::io::{self, BufRead, BufReader, Read};
use std::sync::Arc;

use super::stream::CloudStream;
use super::{AttachRefusal, CloudCursor};

const ATTACH_REFUSED: &str = "the cloud runner refused to attach the chat";
const CARRIAGE_RETURN: char = '\r';
const LINE_END: char = '\n';

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LineOrigin {
    Live,
    Replayed,
}

impl LineOrigin {
    pub(crate) const fn is_replayed(self) -> bool {
        matches!(self, Self::Replayed)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OutputLine {
    pub(crate) text: String,
    pub(crate) origin: LineOrigin,
}

impl OutputLine {
    #[cfg(test)]
    pub(crate) fn live(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            origin: LineOrigin::Live,
        }
    }
}

struct Attached<R> {
    cursor: Arc<CloudCursor>,
    state: Attach<R>,
}

enum Attach<R> {
    Opening(R),
    Open {
        reader: BufReader<CloudStream<R>>,
        handled: Option<JournalOffset>,
    },
    Closed,
}

enum Source<R> {
    Plain(BufReader<R>),
    Cloud(Attached<R>),
}

pub(crate) struct HarnessLines<R> {
    source: Source<R>,
}

impl<R: Read> HarnessLines<R> {
    pub(crate) fn plain(input: R) -> Self {
        Self {
            source: Source::Plain(BufReader::new(input)),
        }
    }

    pub(crate) const fn cloud(input: R, cursor: Arc<CloudCursor>) -> Self {
        Self {
            source: Source::Cloud(Attached {
                cursor,
                state: Attach::Opening(input),
            }),
        }
    }
}

fn read_text(reader: &mut impl BufRead) -> io::Result<Option<String>> {
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    if line.ends_with(LINE_END) {
        line.pop();
        if line.ends_with(CARRIAGE_RETURN) {
            line.pop();
        }
    }
    Ok(Some(line))
}

impl<R: Read> Attached<R> {
    fn next_line(&mut self) -> Option<io::Result<OutputLine>> {
        loop {
            match std::mem::replace(&mut self.state, Attach::Closed) {
                Attach::Closed => return None,
                Attach::Opening(input) => match CloudStream::open(input) {
                    Ok(stream) => {
                        tracing::debug!(
                            from = stream.consumed().0,
                            replay_end = stream.replay_end().0,
                            "attached to a cloud process"
                        );
                        self.state = Attach::Open {
                            reader: BufReader::new(stream),
                            handled: None,
                        };
                    }
                    Err(refusal) => return Some(Err(self.refused(refusal))),
                },
                Attach::Open {
                    mut reader,
                    handled,
                } => {
                    if let Some(offset) = handled {
                        self.cursor.handled_to(offset);
                    }
                    let read = read_text(&mut reader);
                    let stream = reader.get_ref();
                    let origin = if stream.replaying() {
                        LineOrigin::Replayed
                    } else {
                        LineOrigin::Live
                    };
                    let end = JournalOffset(
                        stream
                            .consumed()
                            .0
                            .saturating_sub(reader.buffer().len() as u64),
                    );
                    return match read {
                        Ok(Some(text)) => {
                            self.state = Attach::Open {
                                reader,
                                handled: Some(end),
                            };
                            Some(Ok(OutputLine { text, origin }))
                        }
                        Ok(None) => None,
                        Err(error) => Some(Err(error)),
                    };
                }
            }
        }
    }

    fn refused(&self, refusal: AttachRefusal) -> io::Error {
        tracing::debug!(?refusal, "the cloud runner refused an attach");
        self.cursor.refuse(refusal);
        io::Error::new(io::ErrorKind::ConnectionRefused, ATTACH_REFUSED)
    }
}

impl<R: Read> Iterator for HarnessLines<R> {
    type Item = io::Result<OutputLine>;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.source {
            Source::Plain(reader) => read_text(reader).transpose().map(|read| {
                read.map(|text| OutputLine {
                    text,
                    origin: LineOrigin::Live,
                })
            }),
            Source::Cloud(attached) => attached.next_line(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::CloudRun;
    use krad_domain::Host;
    use krad_host::{HostContext, HostLink};
    use std::path::PathBuf;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn run(from: u64) -> Result<CloudRun, Box<dyn std::error::Error>> {
        Ok(CloudRun::new(
            HostLink::new(
                &Host::Ssh {
                    destination: "runner".parse()?,
                },
                &HostContext::default(),
            ),
            PathBuf::from("/home/me/.krad-ade/bin/krad-ade-agents"),
            krad_domain::Pairing {
                install: "0123456789abcdef".parse()?,
                token: "00112233445566778899aabbccddeeff".parse()?,
            },
            "claude".to_owned(),
            "chat-1".parse()?,
            JournalOffset(from),
        ))
    }

    #[test]
    fn plain_lines_are_live_and_lose_their_line_ends() -> io::Result<()> {
        let lines: Vec<OutputLine> =
            HarnessLines::plain(&b"one\r\ntwo\nthree"[..]).collect::<io::Result<_>>()?;
        assert_eq!(
            lines,
            [
                OutputLine::live("one"),
                OutputLine::live("two"),
                OutputLine::live("three")
            ]
        );
        Ok(())
    }

    #[test]
    fn cloud_lines_mark_the_replay_and_move_the_offset_once_handled() -> TestResult {
        let run = run(4)?;
        let mut lines =
            HarnessLines::cloud(&b"krad-journal 4 12\nold\nold\nnew\n"[..], run.cursor());
        let first = lines.next().transpose()?;
        assert_eq!(first.map(|line| line.origin), Some(LineOrigin::Replayed));
        assert_eq!(run.consumed(), JournalOffset(4));
        let second = lines.next().transpose()?;
        assert_eq!(second.map(|line| line.origin), Some(LineOrigin::Replayed));
        assert_eq!(run.consumed(), JournalOffset(8));
        let third = lines.next().transpose()?;
        assert_eq!(third, Some(OutputLine::live("new")));
        assert_eq!(run.consumed(), JournalOffset(12));
        assert!(lines.next().is_none());
        assert_eq!(run.consumed(), JournalOffset(16));
        assert_eq!(run.refusal(), None);
        Ok(())
    }

    #[test]
    fn a_refused_attach_ends_the_lines_and_tells_the_run_why() -> TestResult {
        let run = run(99)?;
        let mut lines = HarnessLines::cloud(&b"krad-journal-past-end 40\n"[..], run.cursor());
        assert!(lines.next().is_some_and(|line| line.is_err()));
        assert!(lines.next().is_none());
        assert_eq!(
            run.refusal(),
            Some(AttachRefusal::PastEnd {
                end: JournalOffset(40)
            })
        );
        Ok(())
    }
}
