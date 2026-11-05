use gpui_kit::{Context, DummyKeyboardMapper, KeyBinding, KeyBindingContextPredicate};
use krad_extensions::api::manifest::KeybindingContribution;
use krad_extensions::api::QualifiedCommand;
use std::rc::Rc;

use super::super::EnvironmentShell;
use crate::actions::{RunExtensionCommand, SHELL_KEY_CONTEXT};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeyPlatform {
    Macos,
    Linux,
    Windows,
}

const PLATFORM: KeyPlatform = if cfg!(target_os = "macos") {
    KeyPlatform::Macos
} else if cfg!(target_os = "windows") {
    KeyPlatform::Windows
} else {
    KeyPlatform::Linux
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum ClashWith {
    App,
    Extension(QualifiedCommand),
    Unreadable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct KeyConflict {
    pub(in crate::environment_shell) command: QualifiedCommand,
    pub(in crate::environment_shell) keys: String,
    pub(in crate::environment_shell) with: ClashWith,
}

fn platform_keys(binding: &KeybindingContribution, platform: KeyPlatform) -> Option<&str> {
    match platform {
        KeyPlatform::Macos => binding.macos.as_deref(),
        KeyPlatform::Linux => binding.linux.as_deref(),
        KeyPlatform::Windows => binding.windows.as_deref(),
    }
}

fn shell_context() -> Option<Rc<KeyBindingContextPredicate>> {
    KeyBindingContextPredicate::parse(SHELL_KEY_CONTEXT)
        .ok()
        .map(Rc::new)
}

fn load(keys: &str, command: &QualifiedCommand) -> Option<KeyBinding> {
    let action = RunExtensionCommand {
        extension: command.extension.clone(),
        command: command.command.clone(),
    };
    KeyBinding::load(
        keys,
        Box::new(action),
        shell_context(),
        false,
        None,
        &DummyKeyboardMapper,
    )
    .ok()
}

pub(super) struct Planned {
    pub(super) bindings: Vec<(String, QualifiedCommand, KeyBinding)>,
    pub(super) conflicts: Vec<KeyConflict>,
}

pub(super) fn plan(app: &[KeyBinding], wanted: &[(QualifiedCommand, String)]) -> Planned {
    let mut bindings: Vec<(String, QualifiedCommand, KeyBinding)> = Vec::new();
    let mut conflicts = Vec::new();
    for (command, keys) in wanted {
        let Some(binding) = load(keys, command) else {
            conflicts.push(KeyConflict {
                command: command.clone(),
                keys: keys.clone(),
                with: ClashWith::Unreadable,
            });
            continue;
        };
        let clash_app = app
            .iter()
            .any(|known| known.keystrokes() == binding.keystrokes());
        let clash_extension = bindings
            .iter()
            .find(|(_, _, known)| known.keystrokes() == binding.keystrokes())
            .map(|(_, owner, _)| owner.clone());
        let with = match (clash_app, clash_extension) {
            (true, _) => Some(ClashWith::App),
            (false, Some(owner)) => Some(ClashWith::Extension(owner)),
            (false, None) => None,
        };
        match with {
            Some(with) => conflicts.push(KeyConflict {
                command: command.clone(),
                keys: keys.clone(),
                with,
            }),
            None => bindings.push((keys.clone(), command.clone(), binding)),
        }
    }
    Planned {
        bindings,
        conflicts,
    }
}

impl EnvironmentShell {
    pub(super) fn bind_extension_keys(&mut self, cx: &mut Context<Self>) {
        let wanted: Vec<(QualifiedCommand, String)> = self
            .usable_manifests()
            .iter()
            .flat_map(|manifest| {
                manifest
                    .contributes
                    .keybindings
                    .iter()
                    .filter_map(|binding| {
                        let keys = platform_keys(binding, PLATFORM)?;
                        let command = QualifiedCommand {
                            extension: manifest.id.clone(),
                            command: binding.command.clone(),
                        };
                        Some((command, keys.to_owned()))
                    })
            })
            .collect();
        let planned = plan(&crate::keymap::key_bindings(), &wanted);
        self.extensions.conflicts = planned.conflicts;
        let fresh: Vec<KeyBinding> = planned
            .bindings
            .into_iter()
            .filter(|(keys, command, _)| {
                self.extensions.bound.insert((
                    keys.clone(),
                    command.extension.to_string(),
                    command.command.to_string(),
                ))
            })
            .map(|(_, _, binding)| binding)
            .collect();
        if !fresh.is_empty() {
            cx.bind_keys(fresh);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_extensions::api::IdError;

    fn command(extension: &str, command: &str) -> Result<QualifiedCommand, IdError> {
        Ok(QualifiedCommand {
            extension: extension.parse()?,
            command: command.parse()?,
        })
    }

    #[test]
    fn bindings_that_clash_with_the_app_or_an_earlier_extension_are_skipped() -> Result<(), IdError>
    {
        let app = vec![KeyBinding::new(
            "cmd-p",
            crate::actions::TogglePalette,
            None,
        )];
        let wanted = vec![
            (command("docker", "refresh")?, "cmd-p".to_owned()),
            (command("docker", "logs")?, "cmd-alt-l".to_owned()),
            (command("other", "logs")?, "cmd-alt-l".to_owned()),
            (command("other", "broken")?, "cmd-x-y".to_owned()),
        ];
        let planned = plan(&app, &wanted);
        assert_eq!(planned.bindings.len(), 1);
        let clashes: Vec<&ClashWith> = planned
            .conflicts
            .iter()
            .map(|conflict| &conflict.with)
            .collect();
        assert_eq!(clashes.first(), Some(&&ClashWith::App));
        assert_eq!(
            clashes.get(1),
            Some(&&ClashWith::Extension(command("docker", "logs")?))
        );
        assert_eq!(planned.conflicts.len(), 3);
        Ok(())
    }

    #[test]
    fn each_platform_reads_its_own_keys() -> Result<(), IdError> {
        let binding = KeybindingContribution {
            command: "refresh".parse()?,
            macos: Some("cmd-r".to_owned()),
            linux: None,
            windows: Some("ctrl-shift-r".to_owned()),
        };
        assert_eq!(platform_keys(&binding, KeyPlatform::Macos), Some("cmd-r"));
        assert_eq!(platform_keys(&binding, KeyPlatform::Linux), None);
        assert_eq!(
            platform_keys(&binding, KeyPlatform::Windows),
            Some("ctrl-shift-r")
        );
        Ok(())
    }
}
