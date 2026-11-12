use krad_domain::JournalOffset;
use krad_host::runner::{JournalHeader, PastEndNotice};
use std::io::{self, Read};

use super::AttachRefusal;

const LINE_END: u8 = b'\n';
const MAX_OPENING_BYTES: usize = 64 * 1024;

#[derive(Debug)]
pub(crate) struct CloudStream<R> {
    inner: R,
    from: JournalOffset,
    replay_end: JournalOffset,
    consumed: JournalOffset,
}

impl<R: Read> CloudStream<R> {
    pub(crate) fn open(mut inner: R) -> Result<Self, AttachRefusal> {
        let mut skipped = 0;
        while skipped <= MAX_OPENING_BYTES {
            let line = read_opening_line(&mut inner).map_err(|_| AttachRefusal::Refused)?;
            if line.is_empty() {
                return Err(AttachRefusal::Refused);
            }
            let text = String::from_utf8_lossy(&line);
            if let Ok(header) = text.parse::<JournalHeader>() {
                return Ok(Self {
                    inner,
                    from: header.from,
                    replay_end: header.end,
                    consumed: header.from,
                });
            }
            if let Ok(notice) = text.parse::<PastEndNotice>() {
                return Err(AttachRefusal::PastEnd { end: notice.end });
            }
            skipped += line.len();
        }
        Err(AttachRefusal::Refused)
    }

    pub(crate) const fn replay_end(&self) -> JournalOffset {
        self.replay_end
    }

    pub(crate) const fn consumed(&self) -> JournalOffset {
        self.consumed
    }

    pub(crate) fn replaying(&self) -> bool {
        self.consumed <= self.replay_end && self.replay_end > self.from
    }
}

fn read_opening_line(inner: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut line = Vec::new();
    let mut byte = [0; 1];
    while line.len() <= MAX_OPENING_BYTES {
        if inner.read(&mut byte)? == 0 {
            break;
        }
        line.push(byte[0]);
        if byte[0] == LINE_END {
            break;
        }
    }
    Ok(line)
}

impl<R: Read> Read for CloudStream<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let limit = match self.replay_end.0.checked_sub(self.consumed.0) {
            Some(left) if left > 0 => {
                usize::try_from(left).map_or(buf.len(), |left| left.min(buf.len()))
            }
            _ => buf.len(),
        };
        let read = self.inner.read(&mut buf[..limit])?;
        self.consumed = self.consumed.advanced(read);
        Ok(read)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};

    #[test]
    fn the_header_names_the_replay_and_is_not_part_of_the_output() -> io::Result<()> {
        let mut stream = CloudStream::open(&b"krad-journal 10 17\nreplay\nlive\n"[..])
            .map_err(|_| io::Error::other("refused"))?;
        assert_eq!(stream.replay_end(), JournalOffset(17));
        assert_eq!(stream.consumed(), JournalOffset(10));
        assert!(stream.replaying());
        let mut rest = String::new();
        stream.read_to_string(&mut rest)?;
        assert_eq!(rest, "replay\nlive\n");
        assert_eq!(stream.consumed(), JournalOffset(22));
        assert!(!stream.replaying());
        Ok(())
    }

    #[test]
    fn a_read_never_crosses_the_end_of_the_replay() -> io::Result<()> {
        let mut stream = CloudStream::open(&b"krad-journal 0 9\nfirst\nsecond\n"[..])
            .map_err(|_| io::Error::other("refused"))?;
        let mut buf = [0; 64];
        assert_eq!(stream.read(&mut buf)?, 9);
        assert!(stream.replaying());
        assert_eq!(stream.read(&mut buf)?, 4);
        assert!(!stream.replaying());
        Ok(())
    }

    #[test]
    fn lines_read_through_a_buffer_know_whether_they_were_replayed() -> io::Result<()> {
        let stream = CloudStream::open(&b"krad-journal 0 12\nold\nolder\nnew\n"[..])
            .map_err(|_| io::Error::other("refused"))?;
        let mut reader = BufReader::new(stream);
        let mut seen = Vec::new();
        let mut line = String::new();
        while reader.read_line(&mut line)? > 0 {
            seen.push((line.trim_end().to_owned(), reader.get_ref().replaying()));
            line.clear();
        }
        assert_eq!(
            seen,
            [
                ("old".to_owned(), true),
                ("older".to_owned(), true),
                ("new".to_owned(), false)
            ]
        );
        Ok(())
    }

    #[test]
    fn an_attach_from_the_end_replays_nothing() -> io::Result<()> {
        let stream = CloudStream::open(&b"krad-journal 12 12\nnew\n"[..])
            .map_err(|_| io::Error::other("refused"))?;
        assert!(!stream.replaying());
        Ok(())
    }

    #[test]
    fn lines_before_the_header_are_skipped() -> io::Result<()> {
        let stream = CloudStream::open(&b"welcome to the runner\nkrad-journal 0 3\nok\n"[..])
            .map_err(|_| io::Error::other("refused"))?;
        assert_eq!(stream.replay_end(), JournalOffset(3));
        Ok(())
    }

    #[test]
    fn a_refused_attach_says_whether_starting_over_helps() {
        assert_eq!(
            CloudStream::open(&b"krad-journal-past-end 40\n"[..]).err(),
            Some(AttachRefusal::PastEnd {
                end: JournalOffset(40)
            })
        );
        assert_eq!(
            CloudStream::open(&b""[..]).err(),
            Some(AttachRefusal::Refused)
        );
    }
}
