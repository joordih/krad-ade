use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TextEdit {
    pub start: u32,
    pub end: u32,
    pub text: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Order {
    First,
    Second,
}

fn utf16_len(text: &str) -> u32 {
    u32::try_from(text.encode_utf16().count()).unwrap_or(u32::MAX)
}

impl TextEdit {
    #[must_use]
    pub fn new(start: u32, end: u32, text: impl Into<String>) -> Self {
        Self {
            start: start.min(end),
            end: start.max(end),
            text: text.into(),
        }
    }

    #[must_use]
    pub fn inserted_len(&self) -> u32 {
        utf16_len(&self.text)
    }

    const fn is_insert(&self) -> bool {
        self.start == self.end
    }

    fn shift(&self, point: u32) -> u32 {
        let removed = self.end - self.start;
        (point + self.inserted_len()).saturating_sub(removed)
    }

    fn after_text(&self) -> u32 {
        self.start + self.inserted_len()
    }

    fn map_start(&self, point: u32, inserting: bool, order: Order) -> u32 {
        if point < self.start {
            return point;
        }
        if point == self.start {
            return match (self.is_insert(), inserting, order) {
                (true, true, Order::First) | (false, true, _) | (false, false, Order::First) => {
                    point
                }
                (true, _, _) => self.shift(point),
                (false, false, Order::Second) => self.after_text(),
            };
        }
        if point >= self.end {
            return self.shift(point);
        }
        self.after_text()
    }

    fn map_end(&self, point: u32) -> u32 {
        if point <= self.start {
            return point;
        }
        if point >= self.end {
            return self.shift(point);
        }
        self.start
    }

    fn covers(&self, other: &Self, first: bool) -> bool {
        if self.is_insert() || other.start < self.start || other.end > self.end {
            return false;
        }
        if other.is_insert() {
            return self.start < other.start && other.start < self.end;
        }
        let same = self.start == other.start && self.end == other.end;
        !same || first
    }

    #[must_use]
    pub fn transformed_over(&self, applied: &Self, order: Order) -> Self {
        if applied.covers(self, order == Order::Second) {
            return Self::new(applied.start, applied.start, String::new());
        }
        if self.covers(applied, order == Order::First) {
            return Self {
                start: self.start,
                end: applied.shift(self.end),
                text: self.text.clone(),
            };
        }
        let start = applied.map_start(self.start, self.is_insert(), order);
        let end = applied.map_end(self.end).max(start);
        Self {
            start,
            end,
            text: self.text.clone(),
        }
    }

    #[must_use]
    pub fn applied_to(&self, text: &str) -> String {
        let start = byte_index(text, self.start);
        let end = byte_index(text, self.end).max(start);
        let mut edited = String::with_capacity(text.len() + self.text.len());
        edited.push_str(&text[..start]);
        edited.push_str(&self.text);
        edited.push_str(&text[end..]);
        edited
    }
}

fn boundary_at_or_before(text: &str, utf16: u32) -> u32 {
    let mut seen = 0;
    for character in text.chars() {
        let next = seen + u32::try_from(character.len_utf16()).unwrap_or(u32::MAX);
        if next > utf16 {
            return seen;
        }
        seen = next;
    }
    seen
}

impl TextEdit {
    #[must_use]
    pub fn snapped_to(&self, text: &str) -> Self {
        let start = boundary_at_or_before(text, self.start);
        let end = boundary_at_or_before(text, self.end).max(start);
        Self {
            start,
            end,
            text: self.text.clone(),
        }
    }
}

#[must_use]
pub fn byte_index(text: &str, utf16: u32) -> usize {
    let mut seen = 0;
    for (index, character) in text.char_indices() {
        if seen >= utf16 {
            return index;
        }
        seen += u32::try_from(character.len_utf16()).unwrap_or(u32::MAX);
    }
    text.len()
}

#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, Serialize, Deserialize,
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(transparent)]
pub struct BufferVersion(pub u64);

const KEPT_EDITS: usize = 1024;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EditRefusal {
    Stale,
    Ahead,
}

impl std::fmt::Display for EditRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Stale => "the edit is based on a version this buffer no longer keeps",
            Self::Ahead => "the edit is based on a version this buffer has not reached",
        })
    }
}

impl std::error::Error for EditRefusal {}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct VersionedText {
    text: String,
    version: BufferVersion,
    recent: std::collections::VecDeque<TextEdit>,
}

impl VersionedText {
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub const fn version(&self) -> BufferVersion {
        self.version
    }

    /// # Errors
    ///
    /// Refuses an edit based on a version older than the kept edits or newer than this one.
    pub fn apply(
        &mut self,
        based_on: BufferVersion,
        edit: &TextEdit,
    ) -> Result<TextEdit, EditRefusal> {
        let behind = self
            .version
            .0
            .checked_sub(based_on.0)
            .ok_or(EditRefusal::Ahead)?;
        let behind = usize::try_from(behind).map_err(|_| EditRefusal::Stale)?;
        if behind > self.recent.len() {
            return Err(EditRefusal::Stale);
        }
        let skipped = self.recent.len() - behind;
        let rebased = self
            .recent
            .iter()
            .skip(skipped)
            .fold(edit.clone(), |edit, applied| {
                edit.transformed_over(applied, Order::Second)
            });
        let len = utf16_len(&self.text);
        let clamped = TextEdit::new(rebased.start.min(len), rebased.end.min(len), rebased.text);
        let applied = clamped.snapped_to(&self.text);
        self.text = applied.applied_to(&self.text);
        self.version = BufferVersion(self.version.0 + 1);
        self.recent.push_back(applied.clone());
        if self.recent.len() > KEPT_EDITS {
            self.recent.pop_front();
        }
        Ok(applied)
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct EditQueue {
    known: BufferVersion,
    in_flight: bool,
    pending: Vec<TextEdit>,
}

impl EditQueue {
    #[must_use]
    pub fn new(known: BufferVersion) -> Self {
        Self {
            known,
            ..Self::default()
        }
    }

    pub fn push(&mut self, edit: TextEdit) {
        self.pending.push(edit);
    }

    pub fn take_next(&mut self) -> Option<(BufferVersion, TextEdit)> {
        if self.in_flight {
            return None;
        }
        let next = self.pending.first()?.clone();
        self.in_flight = true;
        Some((self.known, next))
    }

    pub fn acknowledged(&mut self) {
        self.known = BufferVersion(self.known.0 + 1);
        if self.in_flight {
            self.pending.remove(0);
            self.in_flight = false;
        }
    }

    #[must_use]
    pub fn heard(&mut self, incoming: &TextEdit) -> TextEdit {
        self.known = BufferVersion(self.known.0 + 1);
        rebase_incoming(&mut self.pending, incoming)
    }

    #[must_use]
    pub fn is_settled(&self) -> bool {
        self.pending.is_empty()
    }
}

fn rebase_incoming(pending: &mut [TextEdit], incoming: &TextEdit) -> TextEdit {
    let mut incoming = incoming.clone();
    for local in pending.iter_mut() {
        let moved = incoming.transformed_over(local, Order::First);
        *local = local.transformed_over(&incoming, Order::Second);
        incoming = moved;
    }
    incoming
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXTS: [&str; 3] = ["abcdef", "añb€c😀d", ""];
    const INSERTS: [&str; 3] = ["", "X", "YZ"];

    fn boundaries(text: &str) -> Vec<u32> {
        let mut points = vec![0];
        for character in text.chars() {
            let last = points.last().copied().unwrap_or(0);
            points.push(last + u32::try_from(character.len_utf16()).unwrap_or(0));
        }
        points
    }

    fn every_edit(text: &str) -> Vec<TextEdit> {
        let points = boundaries(text);
        let mut edits = Vec::new();
        for &start in &points {
            for &end in points.iter().filter(|end| **end >= start) {
                for insert in INSERTS {
                    edits.push(TextEdit::new(start, end, insert));
                }
            }
        }
        edits
    }

    #[test]
    fn concurrent_edits_converge_whichever_side_applies_first() {
        for text in TEXTS {
            let edits = every_edit(text);
            for first in &edits {
                for second in &edits {
                    let first_then = second
                        .transformed_over(first, Order::Second)
                        .applied_to(&first.applied_to(text));
                    let second_then = first
                        .transformed_over(second, Order::First)
                        .applied_to(&second.applied_to(text));
                    assert_eq!(
                        first_then, second_then,
                        "{text:?}: first {first:?}, second {second:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn two_inserts_at_one_point_keep_the_first_one_first() {
        let first = TextEdit::new(1, 1, "A");
        let second = TextEdit::new(1, 1, "B");
        let moved = second.transformed_over(&first, Order::Second);
        assert_eq!(moved.applied_to(&first.applied_to("xy")), "xABy");
    }

    #[test]
    fn utf16_offsets_land_on_character_boundaries() {
        assert_eq!(byte_index("a😀b", 1), 1);
        assert_eq!(byte_index("a😀b", 3), 5);
        assert_eq!(TextEdit::new(1, 3, "e").applied_to("a😀b"), "aeb");
        assert_eq!(
            TextEdit::new(2, 2, "x").snapped_to("a😀b"),
            TextEdit::new(1, 1, "x")
        );
        assert_eq!(utf16_len("a😀b"), 4);
    }

    struct Client {
        text: String,
        queue: EditQueue,
        heard: usize,
    }

    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self, below: u32) -> u32 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            u32::try_from((self.0 >> 33) % u64::from(below.max(1))).unwrap_or(0)
        }
    }

    fn random_edit(text: &str, random: &mut Lcg) -> TextEdit {
        let points = boundaries(text);
        let len = u32::try_from(points.len()).unwrap_or(1);
        let (a, b) = (random.next(len), random.next(len));
        let (start, end) = (points[a as usize], points[b as usize]);
        let insert = ["", "q", "€", "😀w"][random.next(4) as usize];
        TextEdit::new(start.min(end), start.max(end), insert)
    }

    fn hear(client: &mut Client, who: usize, log: &[(usize, TextEdit)]) -> bool {
        let Some((author, applied)) = log.get(client.heard) else {
            return false;
        };
        client.heard += 1;
        if *author == who {
            client.queue.acknowledged();
        } else {
            let incoming = client.queue.heard(applied);
            client.text = incoming.applied_to(&client.text);
        }
        true
    }

    fn send(
        client: &mut Client,
        who: usize,
        buffer: &mut VersionedText,
        log: &mut Vec<(usize, TextEdit)>,
    ) -> Result<(), EditRefusal> {
        if let Some((based, edit)) = client.queue.take_next() {
            log.push((who, buffer.apply(based, &edit)?));
        }
        Ok(())
    }

    #[test]
    fn two_clients_and_the_runtime_converge_on_one_text() -> Result<(), EditRefusal> {
        for seed in 0..300 {
            let mut random = Lcg(seed);
            let mut buffer = VersionedText::new("añb€c😀d");
            let mut log: Vec<(usize, TextEdit)> = Vec::new();
            let mut clients: Vec<Client> = (0..2)
                .map(|_| Client {
                    text: buffer.text().to_owned(),
                    queue: EditQueue::new(buffer.version()),
                    heard: 0,
                })
                .collect();
            for _ in 0..60 {
                let who = random.next(2) as usize;
                let client = &mut clients[who];
                match random.next(3) {
                    0 => {
                        let edit = random_edit(&client.text, &mut random);
                        client.text = edit.applied_to(&client.text);
                        client.queue.push(edit);
                    }
                    1 => send(client, who, &mut buffer, &mut log)?,
                    _ => {
                        hear(client, who, &log);
                    }
                }
            }
            while clients
                .iter()
                .any(|client| !client.queue.is_settled() || client.heard < log.len())
            {
                for (who, client) in clients.iter_mut().enumerate() {
                    send(client, who, &mut buffer, &mut log)?;
                    while hear(client, who, &log) {}
                }
            }
            for (who, client) in clients.iter().enumerate() {
                assert_eq!(client.text, buffer.text(), "seed {seed}, client {who}");
            }
        }
        Ok(())
    }

    #[test]
    fn an_edit_from_the_future_or_from_long_ago_is_refused() {
        let mut buffer = VersionedText::new("text");
        assert_eq!(
            buffer.apply(BufferVersion(3), &TextEdit::new(0, 0, "x")),
            Err(EditRefusal::Ahead)
        );
        for _ in 0..=KEPT_EDITS {
            let version = buffer.version();
            assert!(buffer.apply(version, &TextEdit::new(0, 0, "x")).is_ok());
        }
        assert_eq!(
            buffer.apply(BufferVersion(0), &TextEdit::new(0, 0, "x")),
            Err(EditRefusal::Stale)
        );
    }
}
