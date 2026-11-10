use serde::{Deserialize, Serialize};
use std::fmt;

pub const MAX_MEMORY_LINES: usize = 200;
pub const MAX_MEMORY_LINE_CHARS: usize = 300;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryError {
    Empty,
    TooLong,
    Full,
    NoSuchLine,
}

impl fmt::Display for MemoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("A keepsake needs some text"),
            Self::TooLong => write!(
                f,
                "A keepsake holds at most {MAX_MEMORY_LINE_CHARS} characters"
            ),
            Self::Full => write!(f, "No room for another keepsake ({MAX_MEMORY_LINES} kept)"),
            Self::NoSuchLine => f.write_str("No keepsake there"),
        }
    }
}

impl std::error::Error for MemoryError {}

fn one_line(text: &str) -> Result<String, MemoryError> {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.is_empty() {
        return Err(MemoryError::Empty);
    }
    if line.chars().count() > MAX_MEMORY_LINE_CHARS {
        return Err(MemoryError::TooLong);
    }
    Ok(line)
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KraddieMemory {
    lines: Vec<String>,
}

impl KraddieMemory {
    /// # Errors
    /// Refuses blank or overlong lines and a full memory.
    pub fn add(&mut self, text: &str) -> Result<usize, MemoryError> {
        let line = one_line(text)?;
        if self.lines.len() >= MAX_MEMORY_LINES {
            return Err(MemoryError::Full);
        }
        self.lines.push(line);
        Ok(self.lines.len() - 1)
    }

    /// # Errors
    /// Refuses blank or overlong lines and an index past the end.
    pub fn edit(&mut self, index: usize, text: &str) -> Result<(), MemoryError> {
        let line = one_line(text)?;
        let slot = self.lines.get_mut(index).ok_or(MemoryError::NoSuchLine)?;
        *slot = line;
        Ok(())
    }

    /// # Errors
    /// Refuses an index past the end.
    pub fn remove(&mut self, index: usize) -> Result<String, MemoryError> {
        if index >= self.lines.len() {
            return Err(MemoryError::NoSuchLine);
        }
        Ok(self.lines.remove(index))
    }

    #[must_use]
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    #[must_use]
    pub fn meter(&self) -> String {
        format!("{} of {MAX_MEMORY_LINES} kept", self.lines.len())
    }

    #[must_use]
    pub fn sanitized(self) -> Self {
        let mut memory = Self::default();
        for line in &self.lines {
            if memory.add(line).is_err() && memory.len() >= MAX_MEMORY_LINES {
                break;
            }
        }
        memory
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_added_edited_and_removed() {
        let mut memory = KraddieMemory::default();
        assert_eq!(memory.add("  The api   runs on port 8080 \n"), Ok(0));
        assert_eq!(memory.add("Prefer small PRs"), Ok(1));
        assert_eq!(memory.lines()[0], "The api runs on port 8080");
        assert_eq!(memory.edit(1, "Prefer small pull requests"), Ok(()));
        assert_eq!(memory.edit(5, "x"), Err(MemoryError::NoSuchLine));
        assert_eq!(memory.edit(0, "   "), Err(MemoryError::Empty));
        assert_eq!(memory.remove(0), Ok("The api runs on port 8080".to_owned()));
        assert_eq!(memory.remove(3), Err(MemoryError::NoSuchLine));
        assert_eq!(memory.lines(), ["Prefer small pull requests"]);
        assert_eq!(memory.meter(), "1 of 200 kept");
    }

    #[test]
    fn the_memory_is_bounded_in_lines_and_length() {
        let mut memory = KraddieMemory::default();
        assert_eq!(
            memory.add(&"x".repeat(MAX_MEMORY_LINE_CHARS + 1)),
            Err(MemoryError::TooLong)
        );
        for index in 0..MAX_MEMORY_LINES {
            assert!(memory.add(&format!("fact {index}")).is_ok());
        }
        assert_eq!(memory.add("one more"), Err(MemoryError::Full));
        assert_eq!(memory.len(), MAX_MEMORY_LINES);
    }

    #[test]
    fn a_hand_edited_file_is_sanitized_when_read() -> serde_json::Result<()> {
        let lines: Vec<String> = std::iter::once("  ".to_owned())
            .chain((0..MAX_MEMORY_LINES + 5).map(|index| format!("fact {index}")))
            .collect();
        let text = serde_json::json!({ "lines": lines }).to_string();
        let memory = serde_json::from_str::<KraddieMemory>(&text)?.sanitized();
        assert_eq!(memory.len(), MAX_MEMORY_LINES);
        assert_eq!(memory.lines()[0], "fact 0");
        assert_eq!(
            serde_json::from_str::<KraddieMemory>("{}")?,
            KraddieMemory::default()
        );
        Ok(())
    }
}
