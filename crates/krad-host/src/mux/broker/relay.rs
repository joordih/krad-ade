use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use super::super::bridge::copy_until_end;
use super::super::channel::Channel;
use super::super::session::{Incoming, Session};
use super::super::wire::{OpenRequest, Stream};
use super::pending::Pending;
use crate::remote_link::lock;

const RELAY_THREAD: &str = "krad-ade-mux-relay";
const OPEN_WAIT: Duration = Duration::from_secs(30);
const UNKNOWN_SCRIPT: &str = "the command expired before it started";
const ONLY_COMMANDS: &str = "only commands go through the local broker";

pub(super) fn run(
    pending: &Mutex<Pending>,
    remote: &Session,
    Incoming { channel, request }: Incoming,
) {
    let OpenRequest::Exec { script: id } = request else {
        channel.refuse(ONLY_COMMANDS);
        return;
    };
    let Some(script) = lock(pending).take(&id) else {
        channel.refuse(UNKNOWN_SCRIPT);
        return;
    };
    let spawned = thread::Builder::new().name(RELAY_THREAD.to_owned()).spawn({
        let remote = remote.clone();
        move || match remote.open(OpenRequest::Exec { script }, OPEN_WAIT) {
            Ok((upstream, _)) => {
                if channel.accept(None).is_ok() {
                    splice(&channel, &upstream);
                } else {
                    upstream.kill();
                    upstream.close();
                }
            }
            Err(error) => channel.refuse(&error.to_string()),
        }
    });
    if let Err(error) = spawned {
        tracing::warn!(%error, "could not relay a command");
    }
}

fn splice(local: &Channel, upstream: &Channel) {
    copy_in_background(local.reader(Stream::Main), upstream, Stream::Main);
    let outputs = [Stream::Main, Stream::Errors].map(|stream| {
        let reader = upstream.reader(stream);
        let mut writer = local.writer(stream);
        thread::Builder::new()
            .name(RELAY_THREAD.to_owned())
            .spawn(move || {
                copy_until_end(reader, &mut writer).ok();
                writer.finish();
            })
            .ok()
    });
    let watching = {
        let local = local.clone();
        let upstream = upstream.clone();
        thread::Builder::new()
            .name(RELAY_THREAD.to_owned())
            .spawn(move || {
                local.wait_for_kill();
                if !upstream.is_closed() {
                    upstream.kill();
                }
            })
            .ok()
    };
    for output in outputs.into_iter().flatten() {
        output.join().ok();
    }
    let code = upstream.wait_exit(None).ok().flatten();
    local.exit(code).ok();
    local.close();
    upstream.close();
    if let Some(watching) = watching {
        watching.join().ok();
    }
}

fn copy_in_background(
    reader: impl std::io::Read + Send + 'static,
    upstream: &Channel,
    stream: Stream,
) {
    let mut writer = upstream.writer(stream);
    let spawned = thread::Builder::new()
        .name(RELAY_THREAD.to_owned())
        .spawn(move || {
            copy_until_end(reader, &mut writer).ok();
            writer.finish();
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "could not relay a command's input");
    }
}
