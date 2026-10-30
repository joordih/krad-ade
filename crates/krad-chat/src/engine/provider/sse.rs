use std::io::{self, BufRead};

const DATA_FIELD: &str = "data";
const FIELD_SEPARATOR: char = ':';
const VALUE_LEAD: char = ' ';
const COMMENT_MARK: char = ':';
const DATA_JOINER: char = '\n';
const DONE_MARK: &str = "[DONE]";

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

    fn next_event(&mut self) -> io::Result<Option<String>> {
        let mut data = String::new();
        let mut has_data = false;
        let mut line = String::new();
        loop {
            line.clear();
            if self.reader.read_line(&mut line)? == 0 {
                self.done = true;
                return Ok(has_data.then_some(data));
            }
            let line = line.trim_end_matches(['\r', '\n']);
            if line.is_empty() {
                if has_data {
                    return Ok(Some(data));
                }
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
            if field == DATA_FIELD {
                if has_data {
                    data.push(DATA_JOINER);
                }
                data.push_str(value);
                has_data = true;
            }
        }
    }
}

impl<R: BufRead> Iterator for Events<R> {
    type Item = io::Result<String>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        match self.next_event() {
            Ok(Some(data)) if data.trim() == DONE_MARK => {
                self.done = true;
                None
            }
            Ok(data) => data.map(Ok),
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

    fn events(text: &str) -> Vec<String> {
        Events::new(Cursor::new(text.to_owned()))
            .filter_map(Result::ok)
            .collect()
    }

    #[test]
    fn records_join_their_data_and_skip_comments() {
        let read = events(": ping\nevent: delta\ndata: {\"a\":1}\n\ndata: one\ndata: two\n\n");
        assert_eq!(read, ["{\"a\":1}", "one\ntwo"]);
    }

    #[test]
    fn the_stream_ends_at_done_or_at_the_last_record() {
        assert_eq!(
            events("data: a\r\n\r\ndata: [DONE]\n\ndata: b\n\n").len(),
            1
        );
        assert_eq!(events("data: tail"), ["tail"]);
        assert_eq!(events("event: only\n\n\n"), Vec::<String>::new());
    }
}
