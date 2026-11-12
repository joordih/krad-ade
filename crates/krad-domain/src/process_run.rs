use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(tag = "run", rename_all = "snake_case")]
pub enum ProcessRun {
    #[default]
    Running,
    Exited {
        code: Option<i32>,
    },
    Failed {
        error: String,
    },
}

impl ProcessRun {
    #[must_use]
    pub const fn is_running(&self) -> bool {
        matches!(self, Self::Running)
    }

    pub fn exit(&mut self, code: Option<i32>) {
        if !matches!(self, Self::Failed { .. }) {
            *self = Self::Exited { code };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_exit_ends_a_running_process_but_keeps_a_failure() {
        let mut running = ProcessRun::Running;
        running.exit(Some(1));
        assert_eq!(running, ProcessRun::Exited { code: Some(1) });

        let failure = ProcessRun::Failed {
            error: "writer lost".to_owned(),
        };
        let mut failed = failure.clone();
        failed.exit(Some(0));
        assert_eq!(failed, failure);
    }
}
