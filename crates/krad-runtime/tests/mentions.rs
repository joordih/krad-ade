use futures::executor::block_on;
use krad_chat::new_session_id;
use krad_host::HostLink;
use krad_runtime::{Runtime, RuntimeSetup};
use std::error::Error;
use std::path::{Path, PathBuf};

const LIMIT: usize = 8;

struct Folder(PathBuf);

impl Drop for Folder {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.0));
    }
}

fn found(index: &krad_repo::FileIndex, query: &str) -> Vec<PathBuf> {
    index
        .search(query, LIMIT)
        .into_iter()
        .map(|entry| entry.path.clone())
        .collect()
}

#[test]
fn the_mentions_index_is_built_once_and_built_again_after_it_is_forgotten(
) -> Result<(), Box<dyn Error>> {
    let folder = Folder(std::env::temp_dir().join(format!(
        "krad-runtime-mentions-{}",
        new_session_id().as_str()
    )));
    std::fs::create_dir_all(folder.0.join("src"))?;
    std::fs::write(folder.0.join("src/reader.rs"), "")?;
    let runtime = Runtime::start(RuntimeSetup::default())?;
    let ask = |root: &Path| block_on(runtime.mention_index(HostLink::Local, root.to_path_buf()));
    let first = ask(&folder.0)?.map_err(|error| error.to_string())?;
    assert_eq!(found(&first, "reader"), [PathBuf::from("src/reader.rs")]);
    std::fs::write(folder.0.join("src/writer.rs"), "")?;
    let cached = ask(&folder.0)?.map_err(|error| error.to_string())?;
    assert!(std::sync::Arc::ptr_eq(&first, &cached));
    runtime.forget_mention_indexes();
    let rebuilt = ask(&folder.0)?.map_err(|error| error.to_string())?;
    assert_eq!(found(&rebuilt, "writer"), [PathBuf::from("src/writer.rs")]);
    Ok(())
}
