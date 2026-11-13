use futures::executor::block_on;
use futures::StreamExt as _;
use krad_chat::new_session_id;
use krad_domain::{ChatImage, EnvironmentName, ImageFormat, TerminalId};
use krad_protocol::{
    AgentRef, ChatCommand, Command, ErrorCode, EventKind, Feed, PastedImage, Problem, Reply,
};
use krad_runtime::{EventReceiver, Runtime, RuntimeSetup};
use krad_store::ChatImages;
use std::error::Error;
use std::path::PathBuf;

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
