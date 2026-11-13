use krad_chat::{
    claude_conversation_file, claude_subagents, engine_subagents, read_engine_subagent,
    read_subagent_meta, read_subagent_record, SubagentFiles,
};
use krad_domain::{ResumeId, TranscriptChunk};
use krad_host::HostLink;
use krad_protocol::{KnownRecord, SubagentUpdate, SubagentsAsk, SubagentsFrom, SubagentsRead};
use krad_repo::workplace_home;
use krad_store::gateway_sessions_dir;
use std::io;
use std::path::Path;

fn updates(
    files: Vec<SubagentFiles>,
    known: &[KnownRecord],
    link: &HostLink,
    read: impl Fn(&Path, u64) -> io::Result<TranscriptChunk>,
) -> Vec<SubagentUpdate> {
    files
        .into_iter()
        .filter_map(|files| {
            let (from, finished) = known
                .iter()
                .find(|known| known.record == files.record)
                .map_or((0, false), |known| (known.read, known.finished));
            if finished {
                return None;
            }
            let meta = read_subagent_meta(link, &files.meta).ok()?;
            let chunk = read(&files.record, from).ok()?;
            Some(SubagentUpdate {
                meta,
                record: files.record,
                chunk,
            })
        })
        .collect()
}

fn engine_updates(resume: &ResumeId, cwd: &Path, known: &[KnownRecord]) -> SubagentsRead {
    let Some(sessions) = gateway_sessions_dir() else {
        return SubagentsRead::default();
    };
    SubagentsRead {
        home: None,
        conversation: None,
        updates: updates(
            engine_subagents(&sessions, resume),
            known,
            &HostLink::Local,
            |record, from| read_engine_subagent(record, cwd, from),
        ),
    }
}

#[tracing::instrument(skip_all)]
pub(crate) fn read_subagents(link: &HostLink, ask: SubagentsAsk) -> SubagentsRead {
    let (root, home, conversation) = match ask.from {
        SubagentsFrom::Gateway { cwd } => return engine_updates(&ask.resume, &cwd, &ask.known),
        SubagentsFrom::Claude {
            root,
            home,
            conversation,
        } => (root, home, conversation),
    };
    let home = home.or_else(|| workplace_home(link, root.as_deref()).ok().flatten());
    let conversation = conversation.or_else(|| {
        home.as_deref()
            .and_then(|home| claude_conversation_file(link, home, &ask.resume))
    });
    let updates = conversation.as_deref().map_or_default(|conversation| {
        updates(
            claude_subagents(link, conversation),
            &ask.known,
            link,
            |record, from| read_subagent_record(link, record, from),
        )
    });
    SubagentsRead {
        home,
        conversation,
        updates,
    }
}
