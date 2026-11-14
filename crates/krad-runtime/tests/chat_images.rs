use futures::executor::block_on;
use futures::StreamExt as _;
use krad_chat::new_session_id;
use krad_domain::{
    AgentPreset, ArchivedChat, ChatBookmarks, ChatCheckpoints, ChatChoice, ChatHome, ChatImage,
    EnvironmentData, EnvironmentName, ImageFormat, LooseChat, RuntimeState, SessionEdit,
    TerminalId,
};
use krad_protocol::{
    AgentRef, Answer, ChatCommand, Command, ErrorCode, EventKind, Feed, PastedImage, Problem,
    Reply, RequestId, SessionCommand, Unreadable,
};
use krad_runtime::{EnvironmentsSetup, EventReceiver, Runtime, RuntimeHandle, RuntimeSetup};
use krad_store::ChatImages;
use std::error::Error;
use std::path::PathBuf;
use std::time::SystemTime;

type TestResult = Result<(), Box<dyn Error>>;

const PNG: &[u8] = b"\x89PNG";
const FEED_ENDED: &str = "the feed ended";

struct Folder(PathBuf);

impl Drop for Folder {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.0));
    }
}

fn chat() -> AgentRef {
    AgentRef {
        environment: EnvironmentName::new("main".to_owned()),
        terminal: TerminalId(4),
    }
}

fn attach() -> Command {
    Command::Chat(ChatCommand::AttachImage {
        chat: chat(),
        image: PastedImage {
            format: ImageFormat::Png,
            bytes: PNG.to_vec(),
        },
    })
}

fn attached(feed: &mut EventReceiver) -> Result<ChatImage, Box<dyn Error>> {
    loop {
        match block_on(feed.next()).ok_or(FEED_ENDED)? {
            Feed::Event(event) => match event.kind {
                EventKind::ChatImageAttached { chat: heard, image } if heard == chat() => {
                    return Ok(image)
                }
                EventKind::Problem(Problem { message, .. }) => return Err(message.into()),
                _ => {}
            },
            Feed::Snapshot(_) => {}
        }
    }
}

#[test]
fn a_pasted_image_is_kept_by_the_runtime_and_forgotten_with_its_chat() -> TestResult {
    let folder = Folder(std::env::temp_dir().join(format!(
        "krad-runtime-chat-images-{}",
        new_session_id().as_str()
    )));
    let runtime = Runtime::start(RuntimeSetup {
        chat_images: Some(ChatImages::new(folder.0.clone())),
        ..RuntimeSetup::default()
    })?;
    let mut feed = runtime.subscribe();
    block_on(feed.next()).ok_or(FEED_ENDED)?;
    let id = runtime.next_request();
    assert_eq!(block_on(runtime.send(id, attach()))?, Reply::Accepted);
    let image = attached(&mut feed)?;
    assert!(image.path.starts_with(&folder.0));
    assert_eq!(image.format, ImageFormat::Png);
    assert_eq!(std::fs::read(&image.path)?, PNG);
    let forget = Command::Chat(ChatCommand::ForgetImages { chat: chat() });
    assert_eq!(block_on(runtime.send(id, forget))?, Reply::Accepted);
    block_on(runtime.settle())?;
    while image.path.exists() {
        std::thread::yield_now();
    }
    Ok(())
}

fn answered(feed: &mut EventReceiver, request: RequestId) -> Result<Answer, Box<dyn Error>> {
    loop {
        if let Feed::Event(event) = block_on(feed.next()).ok_or(FEED_ENDED)? {
            if let EventKind::Answered {
                request: heard,
                answer,
            } = event.kind
            {
                if heard == request {
                    return Ok(*answer);
                }
            }
        }
    }
}

fn read_image(runtime: &RuntimeHandle, image: ChatImage) -> Result<RequestId, Box<dyn Error>> {
    let id = runtime.next_request();
    let read = Command::Chat(ChatCommand::ReadImage { image });
    assert_eq!(block_on(runtime.send(id, read))?, Reply::Accepted);
    Ok(id)
}

#[test]
fn a_remote_client_reads_a_kept_image_as_bytes_but_nothing_outside_the_image_folder() -> TestResult
{
    let folder = Folder(std::env::temp_dir().join(format!(
        "krad-runtime-image-bytes-{}",
        new_session_id().as_str()
    )));
    let runtime = Runtime::start(RuntimeSetup {
        chat_images: Some(ChatImages::new(folder.0.clone())),
        ..RuntimeSetup::default()
    })?;
    let mut feed = runtime.subscribe();
    block_on(feed.next()).ok_or(FEED_ENDED)?;
    assert_eq!(
        block_on(runtime.send(runtime.next_request(), attach()))?,
        Reply::Accepted
    );
    let image = attached(&mut feed)?;
    let id = read_image(&runtime, image)?;
    assert_eq!(
        answered(&mut feed, id)?,
        Answer::ImageBytes {
            format: ImageFormat::Png,
            bytes: PNG.to_vec()
        }
    );
    let secret = folder.0.join("secret.png");
    std::fs::write(&secret, PNG)?;
    let outside = ChatImage {
        path: secret,
        format: ImageFormat::Png,
    };
    let id = read_image(&runtime, outside)?;
    assert_eq!(
        answered(&mut feed, id)?,
        Answer::Unreadable {
            unreadable: Unreadable::Outside
        }
    );
    Ok(())
}

#[test]
fn a_runtime_without_an_image_folder_refuses_a_pasted_image() -> TestResult {
    let runtime = Runtime::start(RuntimeSetup::default())?;
    let reply = block_on(runtime.send(runtime.next_request(), attach()))?;
    assert!(matches!(
        reply,
        Reply::Refused {
            code: ErrorCode::Unavailable,
            ..
        }
    ));
    Ok(())
}

#[test]
fn forgetting_an_archive_removes_its_chats_images_under_the_runtime_s_root() -> TestResult {
    let folder = Folder(std::env::temp_dir().join(format!(
        "krad-runtime-archive-files-{}",
        new_session_id().as_str()
    )));
    let mut data = EnvironmentData::empty(chat().environment);
    let group = data.add_group(folder.0.join("repo"), None);
    data.archive.chats.push(LooseChat {
        group,
        home: ChatHome::Gone,
        session_name: "session".to_owned(),
        closed: SystemTime::now(),
        chat: ArchivedChat {
            terminal: chat().terminal,
            title: "chat".to_owned(),
            harness: AgentPreset::ClaudeCode,
            resume: new_session_id(),
            choice: ChatChoice::default(),
            bookmarks: ChatBookmarks::default(),
            checkpoints: ChatCheckpoints::default(),
        },
    });
    let runtime = Runtime::start(RuntimeSetup {
        chat_images: Some(ChatImages::new(folder.0.clone())),
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
    block_on(feed.next()).ok_or(FEED_ENDED)?;
    assert_eq!(
        block_on(runtime.send(runtime.next_request(), attach()))?,
        Reply::Accepted
    );
    let image = attached(&mut feed)?;
    let forget = Command::Session(SessionCommand {
        environment: chat().environment,
        edit: SessionEdit::ForgetArchive,
    });
    assert_eq!(
        block_on(runtime.send(runtime.next_request(), forget))?,
        Reply::Accepted
    );
    while image.path.exists() {
        std::thread::yield_now();
    }
    Ok(())
}
