use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::thread;

use super::channel::Channel;
use super::wire::Stream;

const COPY_THREAD: &str = "krad-ade-mux-copy";
const COPY_BUFFER_BYTES: usize = 32 * 1024;

pub(crate) fn copy_until_end(mut from: impl Read, mut to: impl Write) -> io::Result<u64> {
    let mut buffer = vec![0; COPY_BUFFER_BYTES];
    let mut total = 0;
    loop {
        let read = match from.read(&mut buffer) {
            Ok(0) => return Ok(total),
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        to.write_all(&buffer[..read])?;
        to.flush()?;
        total += u64::try_from(read).unwrap_or(u64::MAX);
    }
}

pub(crate) fn splice_tcp(channel: &Channel, tcp: &TcpStream) -> io::Result<()> {
    let inbound = tcp.try_clone()?;
    let outbound_channel = channel.clone();
    let sending = thread::Builder::new()
        .name(COPY_THREAD.to_owned())
        .spawn(move || {
            let mut writer = outbound_channel.writer(Stream::Main);
            copy_until_end(&inbound, &mut writer).ok();
            writer.finish();
        })?;
    let receiving_tcp = tcp.try_clone()?;
    copy_until_end(channel.reader(Stream::Main), &receiving_tcp).ok();
    tcp.shutdown(Shutdown::Write).ok();
    sending.join().ok();
    tcp.shutdown(Shutdown::Both).ok();
    channel.close();
    Ok(())
}
