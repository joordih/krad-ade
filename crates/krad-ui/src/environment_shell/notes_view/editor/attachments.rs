use gpui_kit::{AppContext as _, ClipboardEntry, Context, Window};
use krad_repo::{add_attachment, save_attachment};
use std::path::PathBuf;

use super::super::super::EnvironmentShell;

const PASTED_IMAGE_STEM: &str = "Pasted image";
const IMAGE_LINK_OPEN: &str = "![](";
const IMAGE_LINK_CLOSE: &str = ")";
const LINK_SPACE: &str = "%20";
const PATH_SEPARATOR: char = '/';
const PARENT_STEP: &str = "../";

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
        let copy = cx.background_spawn(async move {
            files
                .iter()
                .map(|file| add_attachment(&root, file))
                .collect::<Result<Vec<_>, _>>()
        });
        let path = path.to_owned();
        let window_handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            let copied = copy.await;
            cx.update_window(window_handle, |_, window, cx| {
                this.update(cx, |shell, cx| match copied {
                    Ok(links) => shell.insert_links(&path, &links, window, cx),
                    Err(error) => shell.push_toast(error.to_string()),
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
        let save = cx.background_spawn(async move {
            save_attachment(&root, PASTED_IMAGE_STEM, &extension, &bytes)
        });
        let path = path.to_owned();
        let window_handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            let saved = save.await;
            cx.update_window(window_handle, |_, window, cx| {
                this.update(cx, |shell, cx| match saved {
                    Ok(link) => shell.insert_links(&path, &[link], window, cx),
                    Err(error) => shell.push_toast(error.to_string()),
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
