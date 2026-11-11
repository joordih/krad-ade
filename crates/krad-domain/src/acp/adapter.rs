use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ManagedAdapter {
    #[default]
    Unchecked,
    Checking,
    Installing,
    Ready {
        program: PathBuf,
        updating: bool,
    },
    NoNode,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdapterFound {
    Installed { program: PathBuf, update_due: bool },
    Missing,
    NoNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdapterWork {
    Install,
    Update,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdapterStart {
    Check,
    Wait,
    Launch(PathBuf),
    NoNode,
    Failed(String),
}

impl ManagedAdapter {
    pub fn start(&mut self) -> AdapterStart {
        match self {
            Self::Unchecked => {
                *self = Self::Checking;
                AdapterStart::Check
            }
            Self::Checking | Self::Installing => AdapterStart::Wait,
            Self::Ready { program, .. } => AdapterStart::Launch(program.clone()),
            Self::NoNode => AdapterStart::NoNode,
            Self::Failed(reason) => AdapterStart::Failed(reason.clone()),
        }
    }

    pub fn checked(&mut self, found: AdapterFound) -> Option<AdapterWork> {
        if *self != Self::Checking {
            return None;
        }
        match found {
            AdapterFound::Installed {
                program,
                update_due,
            } => {
                *self = Self::Ready {
                    program,
                    updating: update_due,
                };
                update_due.then_some(AdapterWork::Update)
            }
            AdapterFound::Missing => {
                *self = Self::Installing;
                Some(AdapterWork::Install)
            }
            AdapterFound::NoNode => {
                *self = Self::NoNode;
                None
            }
        }
    }

    pub fn finished(&mut self, work: AdapterWork, outcome: Result<PathBuf, String>) {
        match (work, &*self, outcome) {
            (AdapterWork::Install, Self::Installing, Ok(program)) => {
                *self = Self::Ready {
                    program,
                    updating: false,
                };
            }
            (AdapterWork::Install, Self::Installing, Err(reason)) => *self = Self::Failed(reason),
            (AdapterWork::Update, Self::Ready { program, .. }, outcome) => {
                *self = Self::Ready {
                    program: outcome.unwrap_or_else(|_| program.clone()),
                    updating: false,
                };
            }
            (AdapterWork::Install | AdapterWork::Update, _, _) => {}
        }
    }

    pub fn retry(&mut self) {
        if matches!(self, Self::NoNode | Self::Failed(_)) {
            *self = Self::Unchecked;
        }
    }

    #[must_use]
    pub const fn is_installing(&self) -> bool {
        matches!(self, Self::Installing)
    }

    #[must_use]
    pub const fn is_busy(&self) -> bool {
        matches!(self, Self::Checking | Self::Installing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program() -> PathBuf {
        PathBuf::from("adapters/node_modules/.bin/claude-agent-acp")
    }

    #[test]
    fn a_missing_adapter_is_checked_then_installed_then_launched() {
        let mut adapter = ManagedAdapter::default();
        assert_eq!(adapter.start(), AdapterStart::Check);
        assert_eq!(adapter.start(), AdapterStart::Wait);
        assert_eq!(
            adapter.checked(AdapterFound::Missing),
            Some(AdapterWork::Install)
        );
        assert!(adapter.is_installing());
        assert_eq!(adapter.start(), AdapterStart::Wait);
        adapter.finished(AdapterWork::Install, Ok(program()));
        assert_eq!(adapter.start(), AdapterStart::Launch(program()));
    }

    #[test]
    fn an_installed_adapter_launches_at_once_and_updates_only_when_due() {
        let mut fresh = ManagedAdapter::default();
        fresh.start();
        assert_eq!(
            fresh.checked(AdapterFound::Installed {
                program: program(),
                update_due: false
            }),
            None
        );
        assert_eq!(fresh.start(), AdapterStart::Launch(program()));

        let mut stale = ManagedAdapter::default();
        stale.start();
        assert_eq!(
            stale.checked(AdapterFound::Installed {
                program: program(),
                update_due: true
            }),
            Some(AdapterWork::Update)
        );
        assert_eq!(stale.start(), AdapterStart::Launch(program()));
        stale.finished(AdapterWork::Update, Err("offline".to_owned()));
        assert_eq!(
            stale,
            ManagedAdapter::Ready {
                program: program(),
                updating: false
            }
        );
    }

    #[test]
    fn without_node_or_after_a_failed_install_it_stops_until_retried() {
        let mut adapter = ManagedAdapter::default();
        adapter.start();
        assert_eq!(adapter.checked(AdapterFound::NoNode), None);
        assert_eq!(adapter.start(), AdapterStart::NoNode);
        assert_eq!(adapter.start(), AdapterStart::NoNode);
        adapter.retry();
        assert_eq!(adapter.start(), AdapterStart::Check);
        adapter.checked(AdapterFound::Missing);
        adapter.finished(AdapterWork::Install, Err("EACCES".to_owned()));
        assert_eq!(adapter.start(), AdapterStart::Failed("EACCES".to_owned()));
        adapter.retry();
        assert_eq!(adapter, ManagedAdapter::Unchecked);
    }

    #[test]
    fn a_late_answer_does_not_move_a_settled_adapter() {
        let mut adapter = ManagedAdapter::Ready {
            program: program(),
            updating: false,
        };
        assert_eq!(adapter.checked(AdapterFound::Missing), None);
        adapter.finished(AdapterWork::Install, Err("late".to_owned()));
        assert_eq!(adapter.start(), AdapterStart::Launch(program()));
    }
}
