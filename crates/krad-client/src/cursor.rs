use krad_protocol::{Feed, RuntimeRun, Seq, Welcome};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cursor {
    run: Option<RuntimeRun>,
    seq: Option<Seq>,
}

impl Cursor {
    #[must_use]
    pub fn welcomed(&mut self, welcome: &Welcome) -> Option<Seq> {
        if self.run == Some(welcome.run) {
            return self.seq;
        }
        self.run = Some(welcome.run);
        self.seq = None;
        None
    }

    pub fn heard(&mut self, feed: &Feed) {
        let seq = match feed {
            Feed::Snapshot(snapshot) => snapshot.seq,
            Feed::Event(event) => event.seq,
        };
        self.seq = self.seq.max(Some(seq));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_protocol::{Event, EventKind, ProtocolVersion};

    const BUILD: &str = "0.0.0";

    fn welcome(run: u64) -> Welcome {
        Welcome {
            protocol: ProtocolVersion::CURRENT,
            build: BUILD.to_owned(),
            run: RuntimeRun(run),
        }
    }

    fn event(seq: u64) -> Feed {
        Feed::Event(Event {
            seq: Seq(seq),
            kind: EventKind::Unknown,
        })
    }

    #[test]
    fn a_client_resumes_from_its_last_seq_only_with_the_same_run() {
        let mut cursor = Cursor::default();
        assert_eq!(cursor.welcomed(&welcome(1)), None);
        cursor.heard(&event(4));
        cursor.heard(&event(3));
        assert_eq!(cursor.welcomed(&welcome(1)), Some(Seq(4)));
        assert_eq!(cursor.welcomed(&welcome(2)), None);
        assert_eq!(cursor.welcomed(&welcome(2)), None);
        cursor.heard(&event(1));
        assert_eq!(cursor.welcomed(&welcome(2)), Some(Seq(1)));
    }
}
