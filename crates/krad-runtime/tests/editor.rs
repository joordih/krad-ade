#![cfg(unix)]

use futures::executor::block_on;
use futures::StreamExt as _;
use krad_domain::{
    BufferVersion, EditQueue, EnvironmentData, EnvironmentName, RuntimeState, TextEdit,
};
use krad_protocol::{
    BufferEdited, Command, EditorCommand, ErrorCode, EventKind, Feed, OpenedBuffer, Reply,
    RequestId,
};
use krad_runtime::{EnvironmentsSetup, EventReceiver, Runtime, RuntimeHandle, RuntimeSetup};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

type TestResult = Result<(), Box<dyn Error>>;

const ENVIRONMENT: &str = "main";
const FIRST_TEXT: &str = "fn main() {}\n";

fn main_name() -> EnvironmentName {
    EnvironmentName::new(ENVIRONMENT.to_owned())
}

struct Folder(PathBuf);

impl Drop for Folder {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

fn folder(name: &str) -> Result<Folder, Box<dyn Error>> {
    let root = std::env::temp_dir().join(format!("krad-editor-{name}-{}", std::process::id()));
    fs::create_dir_all(&root)?;
    Ok(Folder(root))
}

fn start(root: &Path) -> Result<(RuntimeHandle, EventReceiver), Box<dyn Error>> {
    let mut data = EnvironmentData::empty(main_name());
    data.add_group(root.to_path_buf(), None);
    let runtime = Runtime::start(RuntimeSetup {
        environments: EnvironmentsSetup {
            state: RuntimeState {
                environments: vec![data],
                ..RuntimeState::default()
            },
            file: None,
        },
        ..RuntimeSetup::default()
    })?;
    let mut feed = runtime.subscribe();
    block_on(feed.next()).ok_or("no snapshot")?;
    Ok((runtime, feed))
}

fn send(
    runtime: &RuntimeHandle,
    command: EditorCommand,
) -> Result<(RequestId, Reply), Box<dyn Error>> {
    let request = runtime.next_request();
    let reply = block_on(runtime.send(request, Command::Editor(command)))?;
    Ok((request, reply))
}

fn next_kind(
    feed: &mut EventReceiver,
    mut wanted: impl FnMut(&EventKind) -> bool,
) -> Result<EventKind, Box<dyn Error>> {
    while let Some(feed) = block_on(feed.next()) {
        if let Feed::Event(event) = feed {
            if wanted(&event.kind) {
                return Ok(event.kind);
            }
        }
    }
    Err("the runtime stopped".into())
}

fn opened(feed: &mut EventReceiver) -> Result<OpenedBuffer, Box<dyn Error>> {
    match next_kind(feed, |kind| matches!(kind, EventKind::BufferOpened(_)))? {
        EventKind::BufferOpened(opened) => Ok(*opened),
        _ => Err("not an opened buffer".into()),
    }
}

fn edited(feed: &mut EventReceiver) -> Result<BufferEdited, Box<dyn Error>> {
    match next_kind(feed, |kind| matches!(kind, EventKind::BufferEdited(_)))? {
        EventKind::BufferEdited(edited) => Ok(*edited),
        _ => Err("not an edit".into()),
    }
}

#[test]
fn two_clients_edit_one_buffer_concurrently_and_save_the_merged_text() -> TestResult {
    let folder = folder("merge")?;
    let path = folder.0.join("main.rs");
    fs::write(&path, FIRST_TEXT)?;
    let (runtime, mut feed) = start(&folder.0)?;
    let open = EditorCommand::Open {
        environment: main_name(),
        path: path.clone(),
    };
    assert_eq!(send(&runtime, open.clone())?.1, Reply::Accepted);
    let buffer = opened(&mut feed)?;
    assert_eq!(buffer.text, FIRST_TEXT);
    assert_eq!(send(&runtime, open)?.1, Reply::Accepted);
    assert_eq!(opened(&mut feed)?.buffer, buffer.buffer);

    let mut desktop = EditQueue::new(buffer.version);
    let mut browser = EditQueue::new(buffer.version);
    desktop.push(TextEdit::new(0, 0, "// desktop\n"));
    browser.push(TextEdit::new(11, 11, "\n    run();\n"));
    let (desktop_base, desktop_edit) = desktop.take_next().ok_or("nothing to send")?;
    let (browser_base, browser_edit) = browser.take_next().ok_or("nothing to send")?;
    let edit = |based_on: BufferVersion, edit: TextEdit| EditorCommand::Edit {
        buffer: buffer.buffer,
        based_on,
        edit,
    };
    let (desktop_request, reply) = send(&runtime, edit(desktop_base, desktop_edit))?;
    assert_eq!(reply, Reply::Accepted);
    let (browser_request, reply) = send(&runtime, edit(browser_base, browser_edit))?;
    assert_eq!(reply, Reply::Accepted);
    let first = edited(&mut feed)?;
    let second = edited(&mut feed)?;
    assert_eq!(first.author, desktop_request);
    assert_eq!(second.author, browser_request);
    assert_eq!(second.version, BufferVersion(2));

    let saved_text = "// desktop\nfn main() {\n    run();\n}\n";
    let (_, reply) = send(
        &runtime,
        EditorCommand::Save {
            buffer: buffer.buffer,
        },
    )?;
    assert_eq!(reply, Reply::Accepted);
    next_kind(&mut feed, |kind| {
        matches!(kind, EventKind::BufferSaved { .. })
    })?;
    assert_eq!(fs::read_to_string(&path)?, saved_text);
    let snapshot = block_on(runtime.snapshot(krad_protocol::Scope::Runtime))?;
    assert_eq!(snapshot.buffers[0].saved, BufferVersion(2));
    assert_eq!(snapshot.buffers[0].text, saved_text);
    Ok(())
}

#[test]
fn a_file_that_is_not_text_is_not_opened_and_an_unknown_buffer_is_refused() -> TestResult {
    let folder = folder("binary")?;
    let path = folder.0.join("logo.png");
    fs::write(&path, [0xff, 0xfe, 0x00, 0x81])?;
    let (runtime, mut feed) = start(&folder.0)?;
    let (request, _) = send(
        &runtime,
        EditorCommand::Open {
            environment: main_name(),
            path,
        },
    )?;
    let refused = next_kind(&mut feed, |kind| {
        matches!(kind, EventKind::BufferNotOpened(_))
    })?;
    assert!(matches!(refused, EventKind::BufferNotOpened(not) if not.request == request));
    let (_, reply) = send(
        &runtime,
        EditorCommand::Close {
            buffer: krad_protocol::BufferId(9),
        },
    )?;
    assert!(matches!(
        reply,
        Reply::Refused {
            code: ErrorCode::NotFound,
            ..
        }
    ));
    Ok(())
}
