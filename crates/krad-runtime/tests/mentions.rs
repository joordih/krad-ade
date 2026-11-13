use futures::executor::block_on;
use futures::StreamExt as _;
use krad_domain::{new_session_id, EnvironmentData, EnvironmentName};
use krad_protocol::{
    Answer, ChatCommand, Command, EnvironmentCommand, ErrorCode, EventKind, Feed, Reply,
};
use krad_runtime::{EventReceiver, Runtime, RuntimeHandle, RuntimeSetup};
use std::error::Error;
use std::path::{Path, PathBuf};

const LIMIT: usize = 8;
const ENVIRONMENT: &str = "mentions";

struct Folder(PathBuf);

impl Drop for Folder {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.0));
    }
}

fn environment() -> EnvironmentName {
    EnvironmentName::new(ENVIRONMENT.to_owned())
}

fn mentions(root: &Path, query: &str) -> Command {
    Command::Chat(ChatCommand::Mentions {
        environment: environment(),
        root: root.to_path_buf(),
        query: query.to_owned(),
        limit: LIMIT,
    })
}

fn answer_of(
    runtime: &RuntimeHandle,
    feed: &mut EventReceiver,
    command: Command,
) -> Result<Answer, Box<dyn Error>> {
    let request = runtime.next_request();
    let reply = block_on(runtime.send(request, command))?;
    if reply != Reply::Accepted {
        return Err(format!("{reply:?}").into());
    }
    while let Some(feed) = block_on(feed.next()) {
        if let Feed::Event(event) = feed {
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
    Err("the runtime stopped".into())
}

fn found(answer: Answer) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let Answer::Mentions(found) = answer else {
        return Err(format!("{answer:?}").into());
    };
    Ok(found.entries.into_iter().map(|entry| entry.path).collect())
}

#[test]
fn mentions_are_answered_from_an_index_built_once_and_built_again_after_it_is_forgotten(
) -> Result<(), Box<dyn Error>> {
    let folder = Folder(std::env::temp_dir().join(format!(
        "krad-runtime-mentions-{}",
        new_session_id().as_str()
    )));
    std::fs::create_dir_all(folder.0.join("src"))?;
    std::fs::write(folder.0.join("src/reader.rs"), "")?;
    let runtime = Runtime::start(RuntimeSetup::default())?;
    let mut feed = runtime.subscribe();
    block_on(feed.next()).ok_or("no snapshot")?;
    let unknown = block_on(runtime.send(runtime.next_request(), mentions(&folder.0, "reader")))?;
    assert!(matches!(
        unknown,
        Reply::Refused {
            code: ErrorCode::NotFound,
            ..
        }
    ));
    let mut data = EnvironmentData::empty(environment());
    data.add_group(folder.0.clone(), None);
    let open = Command::Environments(EnvironmentCommand::Open {
        environment: Box::new(data),
    });
    assert_eq!(
        block_on(runtime.send(runtime.next_request(), open))?,
        Reply::Accepted
    );
    let ask = |feed: &mut EventReceiver, query: &str| {
        answer_of(&runtime, feed, mentions(&folder.0, query)).and_then(found)
    };
    assert_eq!(ask(&mut feed, "reader")?, [PathBuf::from("src/reader.rs")]);
    std::fs::write(folder.0.join("src/writer.rs"), "")?;
    assert_eq!(ask(&mut feed, "writer")?, Vec::<PathBuf>::new());
    let forget = Command::Chat(ChatCommand::ForgetMentions);
    assert_eq!(
        block_on(runtime.send(runtime.next_request(), forget))?,
        Reply::Accepted
    );
    assert_eq!(ask(&mut feed, "writer")?, [PathBuf::from("src/writer.rs")]);
    Ok(())
}
