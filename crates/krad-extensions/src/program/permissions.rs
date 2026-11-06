use krad_extension_api::manifest::Permission;
use thiserror::Error;

use crate::program::calls::{AppCall, AppNotice};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum Refusal {
    #[error("the extension did not declare the {0:?} permission")]
    Undeclared(Permission),
}

fn needs(permissions: &[Permission], permission: Permission) -> Result<(), Refusal> {
    if permissions.contains(&permission) {
        Ok(())
    } else {
        Err(Refusal::Undeclared(permission))
    }
}

/// # Errors
///
/// Returns [`Refusal::Undeclared`] when the call needs a permission the extension's manifest
/// does not declare.
pub fn allowed(permissions: &[Permission], call: &AppCall) -> Result<(), Refusal> {
    match call {
        AppCall::OpenTerminal(_) => needs(permissions, Permission::Terminals),
        AppCall::RunCommand(_) => needs(permissions, Permission::RunCommands),
        AppCall::Confirm(_)
        | AppCall::Pick(_)
        | AppCall::InputBox(_)
        | AppCall::WorkspaceContext
        | AppCall::OpenFile(_)
        | AppCall::GetSettings
        | AppCall::GetSecret(_)
        | AppCall::SetSecret(_)
        | AppCall::OpenView(_)
        | AppCall::CloseView(_) => Ok(()),
    }
}

/// # Errors
///
/// Returns [`Refusal::Undeclared`] when the notice needs a permission the extension's manifest
/// does not declare.
pub fn allowed_notice(permissions: &[Permission], notice: &AppNotice) -> Result<(), Refusal> {
    match notice {
        AppNotice::SendTerminal(_) => needs(permissions, Permission::Terminals),
        AppNotice::Log(_)
        | AppNotice::PatchView(_)
        | AppNotice::Toast(_)
        | AppNotice::Progress(_)
        | AppNotice::SetTitlebarItem(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_extension_api::protocol::{
        OpenTerminalParams, RunCommandParams, SendTerminalParams, TerminalCommand, TerminalZone,
    };
    use krad_extension_api::{QualifiedCommand, TerminalHandle};

    fn open_terminal() -> AppCall {
        AppCall::OpenTerminal(OpenTerminalParams {
            title: "shell".to_owned(),
            command: Some(TerminalCommand {
                program: "sh".to_owned(),
                args: Vec::new(),
                env: std::collections::BTreeMap::new(),
            }),
            cwd: None,
            zone: TerminalZone::Session,
        })
    }

    #[test]
    fn terminals_need_their_permission() {
        assert_eq!(
            allowed(&[], &open_terminal()),
            Err(Refusal::Undeclared(Permission::Terminals))
        );
        assert_eq!(allowed(&[Permission::Terminals], &open_terminal()), Ok(()));
        let send = AppNotice::SendTerminal(SendTerminalParams {
            terminal: TerminalHandle(1),
            text: "rm -rf ~\n".to_owned(),
        });
        assert!(allowed_notice(&[], &send).is_err());
        assert!(allowed_notice(&[Permission::Terminals], &send).is_ok());
    }

    #[test]
    fn running_other_extensions_commands_needs_its_permission() -> Result<(), krad_domain::IdError>
    {
        let run = AppCall::RunCommand(RunCommandParams {
            command: QualifiedCommand {
                extension: "git".parse()?,
                command: "push".parse()?,
            },
            target: None,
        });
        assert!(allowed(&[Permission::Terminals], &run).is_err());
        assert!(allowed(&[Permission::RunCommands], &run).is_ok());
        assert_eq!(allowed(&[], &AppCall::WorkspaceContext), Ok(()));
        Ok(())
    }
}
