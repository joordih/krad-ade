use gpui_kit::{AppContext as _, Context};
use krad_lsp::lsp_types::{Position, Uri};
use krad_lsp::{DocumentContentParams, DocumentContentRequest};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash as _, Hasher as _};
use std::path::{Path, PathBuf};

use krad_domain::ToastKind;

use super::super::EnvironmentShell;
use super::{Document, ServerKey};

const LIBRARY_DIRECTORY: &str = "krad-library-documents";
const REMOTE_TEMPORARY_DIRECTORY: &str = "/tmp";
const CLASS_EXTENSION: &str = ".class";
const SOURCE_EXTENSION: &str = ".java";
const URI_SEPARATOR: char = '/';
const OPEN_FAILED_TEXT: &str = "Couldn't open the library source";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LibraryDocument {
    pub(super) key: ServerKey,
    pub(super) uri: Uri,
}

fn file_name(uri: &Uri) -> String {
    let last = uri
        .as_str()
        .rsplit(URI_SEPARATOR)
        .next()
        .unwrap_or_default()
        .to_owned();
    last.strip_suffix(CLASS_EXTENSION)
        .map_or(last.clone(), |stem| format!("{stem}{SOURCE_EXTENSION}"))
}

fn cache_path(temporary: &Path, uri: &Uri) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    uri.as_str().hash(&mut hasher);
    temporary
        .join(LIBRARY_DIRECTORY)
        .join(format!("{:016x}", hasher.finish()))
        .join(file_name(uri))
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn is_library_document(&self, document: &Document) -> bool {
        self.language_servers.library.contains_key(document)
    }

    pub(super) fn open_library_document(
        &mut self,
        from: &Document,
        uri: Uri,
        at: Option<Position>,
        cx: &mut Context<Self>,
    ) {
        let key = self
            .language_servers
            .documents
            .get(from)
            .cloned()
            .or_else(|| {
                self.language_servers
                    .library
                    .get(from)
                    .map(|library| library.key.clone())
            });
        let (Some(key), Some((link, _))) = (key, self.server_link(&from.host)) else {
            return;
        };
        let Some(client) = self
            .language_servers
            .running(&key)
            .map(|running| running.server.client.clone())
        else {
            return;
        };
        let temporary = if link.is_local() {
            std::env::temp_dir()
        } else {
            PathBuf::from(REMOTE_TEMPORARY_DIRECTORY)
        };
        let path = cache_path(&temporary, &uri);
        let asked =
            client.request::<DocumentContentRequest>(&DocumentContentParams { uri: uri.clone() });
        let written = path.clone();
        let writing = cx.background_spawn(async move {
            let content = asked.await.ok().flatten()?;
            let directory = written.parent()?;
            link.create_dir_all(directory).ok()?;
            link.write(&written, content.text.as_bytes()).ok()
        });
        let host = from.host.clone();
        cx.spawn(async move |this, cx| {
            let opened = writing.await;
            this.update(cx, |shell, cx| {
                if opened.is_none() {
                    shell.toast(ToastKind::Error, OPEN_FAILED_TEXT, cx);
                    return;
                }
                shell
                    .language_servers
                    .library
                    .insert(Document::new(&host, &path), LibraryDocument { key, uri });
                shell.open_file_at(&path, at, cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn a_compiled_class_opens_as_a_java_source_in_its_own_folder() {
        let Ok(uri) = Uri::from_str("jrt:///jdk!/java.base/java/lang/String.class") else {
            unreachable!("a literal URI parses");
        };
        let path = cache_path(Path::new("/tmp"), &uri);
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("String.java")
        );
        assert!(path.starts_with("/tmp/krad-library-documents"));
        assert_eq!(cache_path(Path::new("/tmp"), &uri), path);
    }
}
