use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChangeKind {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
    Conflicted,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn change_kinds_keep_their_names_on_the_wire() -> Result<(), serde_json::Error> {
        assert_eq!(
            serde_json::to_string(&ChangeKind::Untracked)?,
            "\"untracked\""
        );
        assert_eq!(
            serde_json::from_str::<ChangeKind>("\"conflicted\"")?,
            ChangeKind::Conflicted
        );
        Ok(())
    }
}
