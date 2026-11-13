use krad_chat::{claude_conversation_file, read_claude_transcript, read_claude_transcript_tail};
use krad_domain::TranscriptChunk;
use krad_host::HostLink;
use krad_protocol::{TranscriptAsk, TranscriptRead};
use krad_repo::workplace_home;
use std::path::Path;
use std::time::SystemTime;

const FIRST_LOOK_BYTES: u64 = 2 * 1024 * 1024;
const NO_HOME_TEXT: &str = "Cannot find the home folder Claude Code keeps conversations in";

fn started_at(link: &HostLink, file: &Path) -> Option<SystemTime> {
    if link.is_local() {
        let metadata = std::fs::metadata(file).ok()?;
        return metadata.created().or_else(|_| metadata.modified()).ok();
    }
    link.metadata(file).ok()?.modified
}

#[tracing::instrument(skip_all)]
pub(crate) fn read_tail(link: &HostLink, ask: TranscriptAsk) -> TranscriptRead {
    let mut read = TranscriptRead {
        resume: ask.resume,
        home: ask.home,
        file: ask.file,
        chunk: TranscriptChunk {
            next: ask.from,
            ..TranscriptChunk::default()
        },
        started: None,
        problem: None,
        start: 0,
    };
    if read.home.is_none() {
        match workplace_home(link, ask.root.as_deref()) {
            Ok(Some(home)) => read.home = Some(home),
            Ok(None) => read.problem = Some(NO_HOME_TEXT.to_owned()),
            Err(error) => read.problem = Some(error.to_string()),
        }
    }
    if read.file.is_none() {
        read.file = read
            .home
            .as_deref()
            .and_then(|home| claude_conversation_file(link, home, &read.resume));
    }
    let Some(file) = read.file.clone() else {
        return read;
    };
    if ask.from == 0 {
        read.started = started_at(link, &file);
    }
    let chunk = if ask.first_look {
        read_claude_transcript_tail(link, &file, FIRST_LOOK_BYTES).map(|(chunk, start)| {
            read.start = start;
            chunk
        })
    } else {
        read_claude_transcript(link, &file, ask.from)
    };
    match chunk {
        Ok(chunk) => read.chunk = chunk,
        Err(error) => read.problem = Some(error.to_string()),
    }
    read
}
