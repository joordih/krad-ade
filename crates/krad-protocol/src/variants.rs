use krad_domain::{
    EnvironmentName, GroupId, VariantId, VariantRemoval, VariantRunId, VariantsOrder, WinnerOutcome,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct VariantCommand {
    pub environment: EnvironmentName,
    pub act: VariantAct,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum VariantAct {
    Run {
        group: GroupId,
        order: Box<VariantsOrder>,
    },
    Pick {
        run: VariantRunId,
        winner: VariantId,
        outcome: WinnerOutcome,
        removal: VariantRemoval,
    },
    Retest {
        run: VariantRunId,
        variant: VariantId,
    },
    CheckWorktrees,
    #[cfg_attr(feature = "ts", ts(skip))]
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Command;

    #[test]
    fn variant_commands_have_a_stable_wire_shape() -> serde_json::Result<()> {
        let pick = Command::Variant(VariantCommand {
            environment: EnvironmentName::new("main".to_owned()),
            act: VariantAct::Pick {
                run: VariantRunId(2),
                winner: VariantId(1),
                outcome: WinnerOutcome::KeepWorktree,
                removal: VariantRemoval::Forced,
            },
        });
        let json = serde_json::to_string(&pick)?;
        assert_eq!(
            json,
            r#"{"area":"variant","environment":"main","act":{"verb":"pick","run":2,"winner":1,"outcome":"keep_worktree","removal":"forced"}}"#
        );
        assert_eq!(serde_json::from_str::<Command>(&json)?, pick);
        let newer: VariantAct = serde_json::from_str(r#"{"verb":"merge_all"}"#)?;
        assert_eq!(newer, VariantAct::Unknown);
        Ok(())
    }
}
