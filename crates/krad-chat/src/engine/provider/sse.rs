use std::io::{self, BufRead};

const DATA_FIELD: &str = "data";
const EVENT_FIELD: &str = "event";
const FIELD_SEPARATOR: char = ':';
const VALUE_LEAD: char = ' ';
const COMMENT_MARK: char = ':';
const DATA_JOINER: char = '\n';
const DONE_MARK: &str = "[DONE]";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Event {
    pub(crate) name: Option<String>,
    pub(crate) data: String,
}

pub(crate) struct Events<R> {
    reader: R,
    done: bool,
}

impl<R: BufRead> Events<R> {
    pub(crate) const fn new(reader: R) -> Self {
        Self {
            reader,
            done: false,
        }
    }

    fn next_event(&mut self) -> io::Result<Option<Event>> {
        let mut event = Event::default();
        let mut has_data = false;
        let mut line = String::new();
        loop {
            line.clear();
            if self.reader.read_line(&mut line)? == 0 {
                self.done = true;
                return Ok(has_data.then_some(event));
            }
            let line = line.trim_end_matches(['\r', '\n']);
            if line.is_empty() {
                if has_data {
                    return Ok(Some(event));
                }
                event.name = None;
                continue;
            }
            if line.starts_with(COMMENT_MARK) {
                continue;
            }
            let (field, value) = line
                .split_once(FIELD_SEPARATOR)
                .map_or((line, ""), |(field, value)| {
                    (field, value.strip_prefix(VALUE_LEAD).unwrap_or(value))
                });
            match field {
                DATA_FIELD => {
                    if has_data {
                        event.data.push(DATA_JOINER);
                    }
                    event.data.push_str(value);
                    has_data = true;
                }
                EVENT_FIELD => event.name = Some(value.to_owned()),
                _ => {}
            }
        }
    }
}

impl<R: BufRead> Iterator for Events<R> {
    type Item = io::Result<Event>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        match self.next_event() {
            Ok(Some(event)) if event.data.trim() == DONE_MARK => {
                self.done = true;
                None
            }
            Ok(event) => event.map(Ok),
            Err(error) => {
                self.done = true;
                Some(Err(error))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn events(text: &str) -> Vec<Event> {
        Events::new(Cursor::new(text.to_owned()))
            .filter_map(Result::ok)
            .collect()
    }

    #[test]
    fn records_carry_their_name_and_joined_data() {
        let read = events(": ping\nevent: delta\ndata: {\"a\":1}\n\ndata: one\ndata: two\n\n");
        assert_eq!(
            read,
            [
                Event {
                    name: Some("delta".to_owned()),
                    data: "{\"a\":1}".to_owned(),
                },
                Event {
                    name: None,
                    data: "one\ntwo".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn the_stream_ends_at_done_or_at_the_last_record() {
        assert_eq!(
            events("data: a\r\n\r\ndata: [DONE]\n\ndata: b\n\n").len(),
            1
        );
        assert_eq!(
            events("data: tail"),
            [Event {
                name: None,
                data: "tail".to_owned(),
            }]
        );
        assert_eq!(events("event: only\n\n\n"), []);
    }
}
