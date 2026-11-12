use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::{
    div, px, rgb, AnyElement, AppContext as _, Context, Entity, FontWeight, InteractiveElement,
    IntoElement, ParentElement, StatefulInteractiveElement, Styled, Subscription, Window,
};
use krad_domain::{EnvironmentName, ToastKind, ToolchainScope};
use krad_host::{HostError, HostLink};
use std::path::{Path, PathBuf};

use super::super::dialog_parts::{
    field_label, sheet_footer, sheet_overlay, sheet_panel, sheet_title,
};
use super::super::EnvironmentShell;
use super::HostDialog;
use crate::components::{icon, Button, ButtonVariant, IconName};
use crate::theme::{
    palette, ICON_SM, LINE_SM, LINE_XS, RADIUS_XS, SPACE_1, SPACE_1_5, SPACE_2, SPACE_5, SPACE_6,
    TEXT_SM, TEXT_XS,
};

const DIALOG_W: f32 = 520.0;
const LIST_MAX_H: f32 = 320.0;
const GROUP_TITLE: &str = "Choose the group's root folder";
const TOOLCHAIN_TITLE: &str = "Choose the toolchain's folder";
const GO_TO_LABEL: &str = "Go to folder";
const GO_TO_PLACEHOLDER: &str = "/absolute/path on the host";
const LOADING_TEXT: &str = "Reading the folder…";
const EMPTY_TEXT: &str = "No folders here";
const NOT_A_FOLDER_TEXT: &str = "That path is not a folder on the host";
const NO_HOME_TEXT: &str = "The host's home folder is not known yet";
const UP_LABEL: &str = "Up";
const CHOOSE_LABEL: &str = "Use this folder";
const CANCEL_LABEL: &str = "Cancel";
const BTN_CLOSE: &str = "folder-browser-close";
const BTN_UP: &str = "folder-browser-up";
const BTN_CANCEL: &str = "folder-browser-cancel";
const BTN_CHOOSE: &str = "folder-browser-choose";
const LIST_ID: &str = "folder-browser-list";
const ROW_ID: &str = "folder-browser-row";

enum Listing {
    Loading,
    Folders(Vec<String>),
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum FolderPurpose {
    NewGroup,
    Toolchain(ToolchainScope),
}

impl FolderPurpose {
    const fn title(&self) -> &'static str {
        match self {
            Self::NewGroup => GROUP_TITLE,
            Self::Toolchain(_) => TOOLCHAIN_TITLE,
        }
    }
}

pub(in crate::environment_shell) struct FolderBrowser {
    purpose: FolderPurpose,
    env: EnvironmentName,
    link: HostLink,
    dir: PathBuf,
    listing: Listing,
    field: Entity<InputState>,
    error: Option<String>,
    _events: Subscription,
}

fn folders_in(link: &HostLink, dir: &Path) -> Result<Vec<String>, HostError> {
    let mut names: Vec<String> = link
        .read_dir(dir)?
        .into_iter()
        .filter(|entry| entry.metadata.is_dir())
        .map(|entry| entry.name)
        .collect();
    names.sort_by_cached_key(|name| name.to_lowercase());
    Ok(names)
}

fn dismiss(this: &mut EnvironmentShell, _: &mut Window, cx: &mut Context<EnvironmentShell>) {
    this.close_host_dialog(cx);
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn open_folder_browser(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_folder_browser_for(FolderPurpose::NewGroup, window, cx);
    }

    pub(in crate::environment_shell) fn open_folder_browser_for(
        &mut self,
        purpose: FolderPurpose,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        let link = self.env_link(&env);
        let HostLink::Remote(ssh) = &link else {
            return;
        };
        let Some(home) = ssh.home().map(Path::to_path_buf) else {
            self.toast(ToastKind::Warning, NO_HOME_TEXT, cx);
            return;
        };
        let field = cx.new(|cx| InputState::new(window, cx).placeholder(GO_TO_PLACEHOLDER));
        let events = cx.subscribe_in(
            &field,
            window,
            |this: &mut Self, field, event: &InputEvent, _, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let typed = field.read(cx).value().trim().to_owned();
                    if !typed.is_empty() {
                        this.browse_to(PathBuf::from(typed), cx);
                    }
                }
            },
        );
        self.hosts.dialog = HostDialog::Folder(FolderBrowser {
            purpose,
            env,
            link,
            dir: home.clone(),
            listing: Listing::Loading,
            field,
            error: None,
            _events: events,
        });
        self.browse_to(home, cx);
    }

    fn browse_to(&mut self, dir: PathBuf, cx: &mut Context<Self>) {
        let HostDialog::Folder(browser) = &mut self.hosts.dialog else {
            return;
        };
        let link = browser.link.clone();
        browser.error = None;
        cx.notify();
        let read = cx.background_spawn({
            let dir = dir.clone();
            async move {
                let metadata = link.metadata(&dir)?;
                if !metadata.is_dir() {
                    return Ok(None);
                }
                folders_in(&link, &dir).map(Some)
            }
        });
        cx.spawn(async move |this, cx| {
            let listed = read.await;
            this.update(cx, |shell, cx| {
                if let HostDialog::Folder(browser) = &mut shell.hosts.dialog {
                    match listed {
                        Ok(Some(folders)) => {
                            browser.dir = dir;
                            browser.listing = Listing::Folders(folders);
                        }
                        Ok(None) => browser.error = Some(NOT_A_FOLDER_TEXT.to_owned()),
                        Err(error) if matches!(browser.listing, Listing::Loading) => {
                            browser.listing = Listing::Failed(error.to_string());
                        }
                        Err(error) => browser.error = Some(error.to_string()),
                    }
                    cx.notify();
                }
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn browse_up(&mut self, cx: &mut Context<Self>) {
        let HostDialog::Folder(browser) = &self.hosts.dialog else {
            return;
        };
        if let Some(parent) = browser.dir.parent().map(Path::to_path_buf) {
            self.browse_to(parent, cx);
        }
    }

    fn choose_browsed_folder(&mut self, cx: &mut Context<Self>) {
        let HostDialog::Folder(browser) = std::mem::take(&mut self.hosts.dialog) else {
            return;
        };
        match browser.purpose {
            FolderPurpose::NewGroup => {
                self.add_group_to(&browser.env, browser.dir, None, cx);
            }
            FolderPurpose::Toolchain(_) => self.check_toolchain_path(browser.dir, cx),
        }
    }

    fn folder_rows(browser: &FolderBrowser, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let pal = palette(cx);
        let note = |text: String| {
            div()
                .px(px(SPACE_2))
                .py(px(SPACE_1_5))
                .text_size(px(TEXT_SM))
                .line_height(px(LINE_SM))
                .text_color(rgb(pal.text_muted))
                .child(text)
                .into_any_element()
        };
        match &browser.listing {
            Listing::Loading => vec![note(LOADING_TEXT.to_owned())],
            Listing::Failed(message) => vec![note(message.clone())],
            Listing::Folders(folders) if folders.is_empty() => vec![note(EMPTY_TEXT.to_owned())],
            Listing::Folders(folders) => folders
                .iter()
                .enumerate()
                .map(|(index, name)| {
                    let target = browser.dir.join(name);
                    div()
                        .id((ROW_ID, index))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(SPACE_1_5))
                        .px(px(SPACE_2))
                        .py(px(SPACE_1))
                        .rounded(px(RADIUS_XS))
                        .cursor_pointer()
                        .hover(|style| style.bg(pal.bg_hover))
                        .text_size(px(TEXT_SM))
                        .line_height(px(LINE_SM))
                        .text_color(rgb(pal.text_default))
                        .child(icon(IconName::Folder, ICON_SM, pal.text_muted))
                        .child(name.clone())
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            this.browse_to(target.clone(), cx);
                        }))
                        .into_any_element()
                })
                .collect(),
        }
    }

    pub(in crate::environment_shell) fn folder_browser_sheet(
        browser: &FolderBrowser,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = Self::folder_rows(browser, cx);
        let pal = palette(cx);
        let current = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .child(
                Button::new(BTN_UP)
                    .label(UP_LABEL)
                    .icon(IconName::ChevronUp)
                    .variant(ButtonVariant::Secondary)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| this.browse_up(cx))),
            )
            .child(
                div()
                    .flex_1()
                    .truncate()
                    .text_size(px(TEXT_SM))
                    .line_height(px(LINE_SM))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(pal.text_strong))
                    .child(browser.dir.to_string_lossy().into_owned()),
            );
        let error = browser.error.clone().map(|message| {
            div()
                .text_size(px(TEXT_XS))
                .line_height(px(LINE_XS))
                .text_color(rgb(pal.error_fg))
                .child(message)
        });
        let body = div()
            .flex()
            .flex_col()
            .gap(px(SPACE_2))
            .px(px(SPACE_6))
            .pb(px(SPACE_5))
            .child(current)
            .child(
                div()
                    .id(LIST_ID)
                    .flex()
                    .flex_col()
                    .max_h(px(LIST_MAX_H))
                    .overflow_y_scroll()
                    .children(rows),
            )
            .child(field_label(GO_TO_LABEL, pal))
            .child(Input::new(&browser.field))
            .children(error);
        let sheet = sheet_panel(DIALOG_W, pal)
            .child(sheet_title(
                browser.purpose.title(),
                Some(browser.link.host().to_string()),
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
                                this.close_host_dialog(cx);
                            })),
                    )
                    .child(
                        Button::new(BTN_CHOOSE)
                            .label(CHOOSE_LABEL)
                            .variant(ButtonVariant::Primary)
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.choose_browsed_folder(cx);
                            })),
                    ),
            );
        sheet_overlay(sheet, dismiss, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn only_folders_are_listed_sorted_without_case() -> TestResult {
        let dir = std::env::temp_dir().join(format!("krad-ui-browser-{}", std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        for folder in ["beta", "Alpha", "gamma"] {
            std::fs::create_dir_all(dir.join(folder))?;
        }
        std::fs::write(dir.join("notes.txt"), "x")?;
        let listed = folders_in(&HostLink::Local, &dir)?;
        std::fs::remove_dir_all(&dir)?;
        assert_eq!(listed, ["Alpha", "beta", "gamma"]);
        Ok(())
    }
}
