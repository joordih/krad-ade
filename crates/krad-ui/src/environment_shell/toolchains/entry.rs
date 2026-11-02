use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::{
    div, px, rgb, AnyElement, AppContext as _, Context, Entity, ParentElement, PathPromptOptions,
    SharedString, Styled, Subscription, Window,
};
use krad_domain::ToolchainScope;
use krad_extensions::{entered_toolchain, SharedLocatorManifest};
use std::path::PathBuf;

use super::super::dialog_parts::{
    field_label, sheet_footer, sheet_overlay, sheet_panel, sheet_title,
};
use super::super::host::FolderPurpose;
use super::super::EnvironmentShell;
use crate::components::{Button, ButtonVariant};
use crate::theme::{palette, LINE_XS, SPACE_2, SPACE_5, SPACE_6, TEXT_XS};

const DIALOG_W: f32 = 480.0;
const PATH_LABEL: &str = "Path";
const PLACEHOLDER: &str = "An interpreter, a virtual environment or an SDK folder";
const SUBTITLE: &str = "The editor analyses this project folder with it";
const CHECKING_TEXT: &str = "Checking the path…";
const EMPTY_TEXT: &str = "Enter a path";
const BROWSE_LABEL: &str = "Browse…";
const USE_LABEL: &str = "Use";
const CANCEL_LABEL: &str = "Cancel";
const BROWSE_PROMPT: &str = "Choose";
const BTN_CLOSE: &str = "toolchain-entry-close";
const BTN_BROWSE: &str = "toolchain-entry-browse";
const BTN_CANCEL: &str = "toolchain-entry-cancel";
const BTN_USE: &str = "toolchain-entry-use";

pub(in crate::environment_shell) struct ToolchainEntry {
    scope: ToolchainScope,
    family_name: String,
    field: Entity<InputState>,
    error: Option<String>,
    checking: bool,
    _events: Subscription,
}

fn dismiss(this: &mut EnvironmentShell, _: &mut Window, cx: &mut Context<EnvironmentShell>) {
    this.close_toolchain_entry(cx);
}

fn not_found(family: &str, path: &str) -> String {
    format!("No {family} was found at {path}")
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn open_toolchain_entry(
        &mut self,
        scope: ToolchainScope,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(family) = self.family_manifest(&scope.family) else {
            return;
        };
        let field = cx.new(|cx| InputState::new(window, cx).placeholder(PLACEHOLDER));
        let focus_target = field.clone();
        cx.defer_in(window, move |_, window, cx| {
            focus_target.update(cx, |field, cx| field.focus(window, cx));
        });
        let events = cx.subscribe_in(
            &field,
            window,
            |this: &mut Self, field, event: &InputEvent, _, cx| match event {
                InputEvent::PressEnter { .. } => {
                    let typed = field.read(cx).value().trim().to_owned();
                    this.check_toolchain_path(PathBuf::from(typed), cx);
                }
                InputEvent::Change => {
                    if let Some(entry) = &mut this.toolchains.entry {
                        entry.error = None;
                    }
                    cx.notify();
                }
                _ => {}
            },
        );
        self.toolchains.entry = Some(ToolchainEntry {
            scope,
            family_name: family.name,
            field,
            error: None,
            checking: false,
            _events: events,
        });
        cx.notify();
    }

    pub(in crate::environment_shell) fn close_toolchain_entry(&mut self, cx: &mut Context<Self>) {
        self.toolchains.entry = None;
        cx.notify();
    }

    pub(in crate::environment_shell) fn check_toolchain_path(
        &mut self,
        path: PathBuf,
        cx: &mut Context<Self>,
    ) {
        let Some(scope) = self
            .toolchains
            .entry
            .as_ref()
            .map(|entry| entry.scope.clone())
        else {
            return;
        };
        let family = self.family_manifest(&scope.family);
        let shared: Vec<SharedLocatorManifest> = self
            .language_tools()
            .map_or_else(Vec::new, |tools| tools.shared_locators().to_vec());
        let inputs = self.discovery_inputs(&scope.host, scope.group_root.clone());
        let Some(entry) = &mut self.toolchains.entry else {
            return;
        };
        if path.as_os_str().is_empty() {
            entry.error = Some(EMPTY_TEXT.to_owned());
            cx.notify();
            return;
        }
        let (Some(family), Some(inputs)) = (family, inputs) else {
            entry.error = Some(not_found(&entry.family_name, &path.display().to_string()));
            cx.notify();
            return;
        };
        entry.checking = true;
        entry.error = None;
        let folder = scope.project_folder.clone();
        let probed = path.clone();
        let checking = cx.background_spawn(async move {
            entered_toolchain(&family, &shared, &inputs.request(&folder), &probed)
        });
        cx.spawn(async move |this, cx| {
            let found = checking.await;
            this.update(cx, |shell, cx| {
                let Some(entry) = &mut shell.toolchains.entry else {
                    return;
                };
                entry.checking = false;
                match found {
                    Some(toolchain) => {
                        shell.toolchains.entry = None;
                        shell.choose_toolchain(&scope, toolchain, cx);
                    }
                    None => {
                        entry.error =
                            Some(not_found(&entry.family_name, &path.display().to_string()));
                    }
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
        cx.notify();
    }

    fn browse_toolchain(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = &self.toolchains.entry else {
            return;
        };
        let scope = entry.scope.clone();
        if !scope.host.is_local() {
            self.open_folder_browser_for(FolderPurpose::Toolchain(scope), window, cx);
            return;
        }
        let answer = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: true,
            multiple: false,
            prompt: Some(SharedString::from(BROWSE_PROMPT)),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = answer.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            this.update(cx, |shell, cx| shell.check_toolchain_path(path, cx))
                .unwrap_or_default();
        })
        .detach();
    }

    pub(in crate::environment_shell) fn toolchain_entry_sheet(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let entry = self.toolchains.entry.as_ref()?;
        let pal = palette(cx);
        let note = entry
            .error
            .clone()
            .map(|error| (error, pal.error_fg))
            .or_else(|| {
                entry
                    .checking
                    .then(|| (CHECKING_TEXT.to_owned(), pal.text_muted))
            })
            .map(|(text, color)| {
                div()
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(color))
                    .child(text)
            });
        let field = entry.field.clone();
        let body = div()
            .flex()
            .flex_col()
            .gap(px(SPACE_2))
            .px(px(SPACE_6))
            .pb(px(SPACE_5))
            .child(field_label(PATH_LABEL, pal))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(SPACE_2))
                    .child(div().flex_1().child(Input::new(&entry.field)))
                    .child(
                        Button::new(BTN_BROWSE)
                            .label(BROWSE_LABEL)
                            .variant(ButtonVariant::Secondary)
                            .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                                this.browse_toolchain(window, cx);
                            })),
                    ),
            )
            .children(note);
        let sheet = sheet_panel(DIALOG_W, pal)
            .child(sheet_title(
                format!("Use a {} toolchain", entry.family_name),
                Some(SUBTITLE.to_owned()),
                BTN_CLOSE,
                dismiss,
                cx,
            ))
            .child(body)
            .child(
                sheet_footer(pal)
                    .justify_end()
                    .child(
                        Button::new(BTN_CANCEL)
                            .label(CANCEL_LABEL)
                            .variant(ButtonVariant::Secondary)
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.close_toolchain_entry(cx);
                            })),
                    )
                    .child(
                        Button::new(BTN_USE)
                            .label(USE_LABEL)
                            .variant(ButtonVariant::Primary)
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                let typed = field.read(cx).value().trim().to_owned();
                                this.check_toolchain_path(PathBuf::from(typed), cx);
                            })),
                    ),
            );
        Some(sheet_overlay(sheet, dismiss, cx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rejected_path_says_which_family_was_looked_for() {
        assert_eq!(
            not_found("Python", "/srv/app/env"),
            "No Python was found at /srv/app/env"
        );
    }
}
