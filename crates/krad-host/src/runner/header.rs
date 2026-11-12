use krad_domain::JournalOffset;
use std::fmt;
use std::str::FromStr;

const HEADER_TAG: &str = "krad-journal";
const PAST_END_TAG: &str = "krad-journal-past-end";
const FIELD_SEPARATOR: char = ' ';

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JournalHeader {
    pub from: JournalOffset,
    pub end: JournalOffset,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("a cloud attach starts with 'krad-journal <from> <end>'")]
pub struct InvalidJournalHeader;

impl fmt::Display for JournalHeader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{HEADER_TAG}{FIELD_SEPARATOR}{}{FIELD_SEPARATOR}{}",
            self.from.0, self.end.0
        )
    }
}

impl FromStr for JournalHeader {
    type Err = InvalidJournalHeader;

    fn from_str(line: &str) -> Result<Self, Self::Err> {
        let mut fields = line.trim_end().split(FIELD_SEPARATOR);
        let (Some(HEADER_TAG), Some(from), Some(end), None) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            return Err(InvalidJournalHeader);
        };
        Ok(Self {
            from: JournalOffset(from.parse().map_err(|_| InvalidJournalHeader)?),
            end: JournalOffset(end.parse().map_err(|_| InvalidJournalHeader)?),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PastEndNotice {
    pub end: JournalOffset,
}

impl fmt::Display for PastEndNotice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{PAST_END_TAG}{FIELD_SEPARATOR}{}", self.end.0)
    }
}

impl FromStr for PastEndNotice {
    type Err = InvalidJournalHeader;

    fn from_str(line: &str) -> Result<Self, Self::Err> {
        let mut fields = line.trim_end().split(FIELD_SEPARATOR);
        let (Some(PAST_END_TAG), Some(end), None) = (fields.next(), fields.next(), fields.next())
        else {
            return Err(InvalidJournalHeader);
        };
        Ok(Self {
            end: JournalOffset(end.parse().map_err(|_| InvalidJournalHeader)?),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_header_names_where_the_replay_starts_and_ends() {
        let header = JournalHeader {
            from: JournalOffset(4),
            end: JournalOffset(90),
        };
        assert_eq!(header.to_string(), "krad-journal 4 90");
        assert_eq!("krad-journal 4 90\n".parse(), Ok(header));
        assert!("krad-journal 4".parse::<JournalHeader>().is_err());
        assert!("journal 4 90".parse::<JournalHeader>().is_err());
        assert!("krad-journal-past-end 90".parse::<JournalHeader>().is_err());
    }

    #[test]
    fn a_refused_offset_names_where_the_journal_ends() {
        let notice = PastEndNotice {
            end: JournalOffset(12),
        };
        assert_eq!(notice.to_string(), "krad-journal-past-end 12");
        assert_eq!("krad-journal-past-end 12\n".parse(), Ok(notice));
        assert!("krad-journal 0 12".parse::<PastEndNotice>().is_err());
    }
}
