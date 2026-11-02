use std::io::{self, Read, Write};

use super::super::wire::Stream;
use super::Channel;

pub(crate) struct ChannelReader {
    pub(super) channel: Channel,
    pub(super) stream: Stream,
}

impl Read for ChannelReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.channel.read(self.stream, buf)
    }
}

pub(crate) struct ChannelWriter {
    pub(super) channel: Channel,
    pub(super) stream: Stream,
    pub(super) ended: bool,
}

impl ChannelWriter {
    pub(crate) fn finish(&mut self) {
        if !std::mem::replace(&mut self.ended, true) {
            self.channel.end(self.stream);
        }
    }
}

impl Write for ChannelWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.channel
            .write_all(self.stream, buf)
            .map_err(io::Error::from)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for ChannelWriter {
    fn drop(&mut self) {
        self.finish();
    }
}
