use krad_domain::ChatImage;
use krad_protocol::{
    AgentRef, Answer, ErrorCode, EventKind, PastedImage, Problem, ProblemScope, Reply, RequestId,
    Unreadable,
};
use krad_store::ImageUnread;

use super::{refused, Runtime};
use crate::jobs::JobDone;

const NO_IMAGE_FOLDER: &str = "this runtime keeps no pasted images";
const NO_JOB: &str = "the runtime could not start the image job";
const IMAGE_BYTES_LIMIT: u64 = 1024 * 1024;

const fn unreadable(unread: &ImageUnread) -> Unreadable {
    match *unread {
        ImageUnread::Missing => Unreadable::Missing,
        ImageUnread::Outside => Unreadable::Outside,
        ImageUnread::TooLarge { len, limit } => Unreadable::TooLarge { len, limit },
    }
}

#[derive(Debug)]
pub(crate) enum ChatImageDone {
    Saved {
        chat: AgentRef,
        saved: Result<ChatImage, String>,
    },
    Forgotten {
        chat: AgentRef,
        forgotten: Result<(), String>,
    },
}

impl Runtime {
    pub(super) fn attach_chat_image(&mut self, chat: AgentRef, image: PastedImage) -> Reply {
        let Some(images) = self.chat_images.clone() else {
            return refused(ErrorCode::Unavailable, NO_IMAGE_FOLDER);
        };
        let saving = self.jobs.run(move || {
            let saved = images
                .save(&chat.environment, chat.terminal, image.format, &image.bytes)
                .map_err(|error| error.to_string());
            JobDone::ChatImage(ChatImageDone::Saved { chat, saved })
        });
        saving.map_or_else(
            |_| refused(ErrorCode::Unavailable, NO_JOB),
            |()| Reply::Accepted,
        )
    }

    pub(super) fn read_chat_image(&self, request: RequestId, image: ChatImage) -> Reply {
        let Some(images) = self.chat_images.clone() else {
            return refused(ErrorCode::Unavailable, NO_IMAGE_FOLDER);
        };
        self.answer_in_job(request, move || {
            match images.read(&image, IMAGE_BYTES_LIMIT) {
                Ok(bytes) => Answer::ImageBytes {
                    format: image.format,
                    bytes,
                },
                Err(unread) => Answer::Unreadable {
                    unreadable: unreadable(&unread),
                },
            }
        })
    }

    pub(super) fn forget_chat_images(&mut self, chat: AgentRef) -> Reply {
        let Some(images) = self.chat_images.clone() else {
            return Reply::Accepted;
        };
        let forgetting = self.jobs.run(move || {
            let forgotten = images
                .forget(&chat.environment, chat.terminal)
                .map_err(|error| error.to_string());
            JobDone::ChatImage(ChatImageDone::Forgotten { chat, forgotten })
        });
        forgetting.map_or_else(
            |_| refused(ErrorCode::Unavailable, NO_JOB),
            |()| Reply::Accepted,
        )
    }

    pub(super) fn chat_image_done(&mut self, done: ChatImageDone) {
        let (chat, failed) = match done {
            ChatImageDone::Saved {
                chat,
                saved: Ok(image),
            } => {
                self.outbox
                    .push(EventKind::ChatImageAttached { chat, image });
                return;
            }
            ChatImageDone::Forgotten {
                forgotten: Ok(()), ..
            } => return,
            ChatImageDone::Saved {
                chat,
                saved: Err(message),
            }
            | ChatImageDone::Forgotten {
                chat,
                forgotten: Err(message),
            } => (chat, message),
        };
        self.outbox.push(EventKind::Problem(Problem {
            scope: ProblemScope::Chat { chat },
            code: ErrorCode::Unavailable,
            message: failed,
        }));
    }
}
