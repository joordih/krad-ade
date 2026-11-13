use gpui_kit::{AppContext as _, ClipboardEntry, Context, Window};
use krad_client::ClientError;
use krad_protocol::{ErrorCode, FileAsk};
use std::path::{Path, PathBuf};

use super::super::super::file_asks::vault_path_of;
use super::super::super::EnvironmentShell;

const PASTED_IMAGE_STEM: &str = "Pasted image";
const IMAGE_LINK_OPEN: &str = "![](";
const IMAGE_LINK_CLOSE: &str = ")";
const LINK_SPACE: &str = "%20";
const PATH_SEPARATOR: char = '/';
const PARENT_STEP: &str = "../";

const NO_NAME_TEXT: &str = "The dropped file has no name";

fn attachment_of(root: &Path, file: &Path) -> Result<FileAsk, ClientError> {
    let failed = |message: String| ClientError::Failed {
        code: ErrorCode::Invalid,
        message,
    };
    let stem = file
        .file_stem()
        .ok_or_else(|| failed(NO_NAME_TEXT.to_owned()))?
        .to_string_lossy()
        .into_owned();
    let extension = file
        .extension()
        .map_or_default(|extension| extension.to_string_lossy().into_owned());
    let bytes = std::fs::read(file).map_err(|error| failed(error.to_string()))?;
    Ok(FileAsk::SaveAttachment {
        root: root.to_path_buf(),
        stem,
        extension,
        bytes,
    })
}

fn attachment_link(note: &str, path: &str) -> String {
    let depth = note.matches(PATH_SEPARATOR).count();
    let relative = format!("{}{path}", PARENT_STEP.repeat(depth));
    format!(
        "{IMAGE_LINK_OPEN}{}{IMAGE_LINK_CLOSE}",
        relative.replace(' ', LINK_SPACE)
    )
}

impl EnvironmentShell {
    pub(super) fn attach_files(
        &mut self,
        path: &str,
        files: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self.notes.root.clone() else {
            return;
        };
        let read = cx.background_spawn(async move {
            files
                .iter()
                .map(|file| attachment_of(&root, file))
                .collect::<Result<Vec<_>, _>>()
        });
        let path = path.to_owned();
        let window_handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            let mut copied = read.await.map(|asks| (asks, Vec::new()));
            if let Ok((asks, links)) = &mut copied {
                for ask in std::mem::take(asks) {
                    let Ok(saved) = this.update(cx, |shell, cx| {
                        shell.ask_active_file(ask, vault_path_of, cx)
                    }) else {
                        return;
                    };
                    match saved.await {
                        Ok(link) => links.push(link),
                        Err(error) => {
                            copied = Err(error);
                            break;
                        }
                    }
                }
            }
            let copied = copied.map(|(_, links)| links);
            cx.update_window(window_handle, |_, window, cx| {
                this.update(cx, |shell, cx| match copied {
                    Ok(links) => shell.insert_links(&path, &links, window, cx),
                    Err(error) => shell.toast_failure(&error, cx),
                })
                .unwrap_or_default();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn insert_links(
        &mut self,
        path: &str,
        attachments: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self
            .notes
            .editors
            .get(path)
            .and_then(|editor| editor.state.clone())
        else {
            return;
        };
        let links: Vec<String> = attachments
            .iter()
            .map(|attachment| attachment_link(path, attachment))
            .collect();
        state.update(cx, |state, cx| state.insert(links.join("\n"), window, cx));
        self.refresh_vault(cx);
    }

    pub(super) fn paste_image(
        &mut self,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(root) = self.notes.root.clone() else {
            return false;
        };
        let image = cx.read_from_clipboard().and_then(|item| {
            item.entries().iter().find_map(|entry| match entry {
                ClipboardEntry::Image(image) => Some(image.clone()),
                _ => None,
            })
        });
        let Some(image) = image else {
            return false;
        };
        let extension = image.format.extension().to_owned();
        let bytes = image.bytes.clone();
        let ask = FileAsk::SaveAttachment {
            root,
            stem: PASTED_IMAGE_STEM.to_owned(),
            extension,
            bytes,
        };
        let save = self.ask_active_file(ask, vault_path_of, cx);
        let path = path.to_owned();
        let window_handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            let saved = save.await;
            cx.update_window(window_handle, |_, window, cx| {
                this.update(cx, |shell, cx| match saved {
                    Ok(link) => shell.insert_links(&path, &[link], window, cx),
                    Err(error) => shell.toast_failure(&error, cx),
                })
                .unwrap_or_default();
            })
            .unwrap_or_default();
        })
        .detach();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_links_keep_spaces_inside_the_link() {
        assert_eq!(
            attachment_link("Plan.md", "attachments/Pasted image 2.png"),
            "![](attachments/Pasted%20image%202.png)"
        );
        assert_eq!(
            attachment_link("Ideas/2026/Spark.md", "attachments/a.png"),
            "![](../../attachments/a.png)"
        );
    }
}
