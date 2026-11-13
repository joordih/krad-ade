use krad_domain::{
    Automation, AutomationKey, DigestBook, Kraddie, KraddieActivity, KraddieHistory, KraddieId,
    KraddieMind, MemoryOwner,
};
use krad_store::{KraddieRename, KraddieShelf, OrdinaryDigests};
use std::fmt;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::thread;

const WRITER_THREAD: &str = "krad-runtime-kraddies";
const WRITE_QUEUE: usize = 256;

pub(super) struct Renamed {
    pub(super) old: KraddieId,
    pub(super) kraddie: Kraddie,
    pub(super) automations: Vec<Automation>,
    pub(super) histories: Vec<(AutomationKey, KraddieHistory)>,
}

pub(super) enum ShelfWrite {
    Kraddie(Box<Kraddie>),
    Automations(KraddieId, Vec<Automation>),
    History(AutomationKey, KraddieHistory),
    ForgetHistory(AutomationKey),
    Mind(KraddieId, KraddieMind),
    Activity(KraddieId, KraddieActivity),
    Digests(MemoryOwner, DigestBook),
    Rename(Box<Renamed>),
    Delete(KraddieId),
    Then(Box<dyn FnOnce() + Send>),
}

impl fmt::Debug for ShelfWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self {
            Self::Kraddie(_) => "kraddie",
            Self::Automations(..) => "automations",
            Self::History(..) => "history",
            Self::ForgetHistory(_) => "forget_history",
            Self::Mind(..) => "mind",
            Self::Activity(..) => "activity",
            Self::Digests(..) => "digests",
            Self::Rename(_) => "rename",
            Self::Delete(_) => "delete",
            Self::Then(_) => "then",
        };
        formatter.debug_tuple("ShelfWrite").field(&kind).finish()
    }
}

#[derive(Debug)]
pub(super) struct ShelfWriter(SyncSender<ShelfWrite>);

struct Files {
    shelf: Option<KraddieShelf>,
    ordinary: Option<OrdinaryDigests>,
}

impl ShelfWriter {
    pub(super) fn spawn(
        shelf: Option<KraddieShelf>,
        ordinary: Option<OrdinaryDigests>,
    ) -> Option<Self> {
        let (sender, receiver) = sync_channel(WRITE_QUEUE);
        let files = Files { shelf, ordinary };
        thread::Builder::new()
            .name(WRITER_THREAD.to_owned())
            .spawn(move || files.serve(&receiver))
            .inspect_err(|error| tracing::warn!(%error, "the Kraddie writer could not start"))
            .ok()?;
        Some(Self(sender))
    }

    pub(super) fn send(&self, write: ShelfWrite) {
        if let Err(unsent) = self.0.send(write) {
            tracing::warn!(write = ?unsent.0, "the Kraddie writer stopped; a change was not saved");
        }
    }
}

impl Files {
    fn serve(&self, receiver: &Receiver<ShelfWrite>) {
        for write in receiver {
            self.write(write);
        }
    }

    fn write(&self, write: ShelfWrite) {
        let kind = format!("{write:?}");
        let written = match write {
            ShelfWrite::Then(then) => {
                then();
                Ok(())
            }
            ShelfWrite::Digests(MemoryOwner::Ordinary, book) => self
                .ordinary
                .as_ref()
                .map_or(Ok(()), |files| files.write(&book))
                .map_err(|error| error.to_string()),
            write => self.shelf_write(write),
        };
        if let Err(error) = written {
            tracing::warn!(write = kind, %error, "a Kraddie change was not saved");
        }
    }

    fn shelf_write(&self, write: ShelfWrite) -> Result<(), String> {
        let Some(files) = &self.shelf else {
            return Ok(());
        };
        let written = match write {
            ShelfWrite::Kraddie(kraddie) => files.write(&kraddie),
            ShelfWrite::Automations(id, automations) => files.write_automations(&id, &automations),
            ShelfWrite::History(key, history) => files.write_history(&key, &history),
            ShelfWrite::ForgetHistory(key) => files.delete_history(&key),
            ShelfWrite::Mind(id, mind) => files
                .write_compass(&id, &mind.compass)
                .and_then(|()| files.write_memory(&id, &mind.memory)),
            ShelfWrite::Activity(id, activity) => files.write_activity(&id, &activity),
            ShelfWrite::Digests(MemoryOwner::Kraddie(id), book) => {
                return files
                    .write_digests(&id, &book)
                    .map_err(|error| error.to_string());
            }
            ShelfWrite::Rename(renamed) => files.rename(&KraddieRename {
                old: &renamed.old,
                kraddie: &renamed.kraddie,
                automations: &renamed.automations,
                histories: &renamed.histories,
            }),
            ShelfWrite::Delete(id) => files.delete(&id),
            ShelfWrite::Digests(MemoryOwner::Ordinary, _) | ShelfWrite::Then(_) => Ok(()),
        };
        written.map_err(|error| error.to_string())
    }
}
