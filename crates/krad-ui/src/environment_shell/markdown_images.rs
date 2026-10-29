use gpui_kit::base::TextView;
use gpui_kit::{ImageSource, SharedUri};
use std::path::{Path, PathBuf};

const FILE_SCHEME: &str = "file://";
const DATA_SCHEME: &str = "data:";
const SCHEME_SEPARATOR: &str = "://";
const DATA_IMAGE_REFERENCE: &str = "(data:";

fn local_image(folder: &Path, url: &str) -> Option<PathBuf> {
    if let Some(path) = url.strip_prefix(FILE_SCHEME) {
        return Some(PathBuf::from(path));
    }
    if url.starts_with(DATA_SCHEME) || url.contains(SCHEME_SEPARATOR) {
        return None;
    }
    let path = Path::new(url);
    Some(if path.is_absolute() {
        path.to_path_buf()
    } else {
        folder.join(path)
    })
}

pub(super) fn with_local_images(view: TextView, folder: &Path, markdown: &str) -> TextView {
    if markdown.contains(DATA_IMAGE_REFERENCE) {
        return view;
    }
    let folder = folder.to_path_buf();
    view.image_source(move |url: &SharedUri| match local_image(&folder, url) {
        Some(path) => ImageSource::from(path),
        None => ImageSource::from(url.clone()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_image_resolves_against_the_document_folder() {
        let folder = Path::new("/repo/docs");
        assert_eq!(
            local_image(folder, "img/flow.png"),
            Some(PathBuf::from("/repo/docs/img/flow.png"))
        );
        assert_eq!(
            local_image(folder, "../assets/logo.svg"),
            Some(PathBuf::from("/repo/docs/../assets/logo.svg"))
        );
    }

    #[test]
    fn absolute_and_file_urls_load_from_disk() {
        let folder = Path::new("/repo/docs");
        assert_eq!(
            local_image(folder, "/tmp/shot.png"),
            Some(PathBuf::from("/tmp/shot.png"))
        );
        assert_eq!(
            local_image(folder, "file:///tmp/shot.png"),
            Some(PathBuf::from("/tmp/shot.png"))
        );
    }

    #[test]
    fn web_and_inline_images_are_not_local() {
        let folder = Path::new("/repo/docs");
        assert_eq!(local_image(folder, "https://example.com/badge.svg"), None);
        assert_eq!(
            local_image(folder, "data:image/png;base64,iVBORw0KGgo="),
            None
        );
    }
}
