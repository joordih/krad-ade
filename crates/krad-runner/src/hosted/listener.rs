use krad_host::runner::{ensure_private_dir, runtime_socket};
use std::fs;
use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;

pub trait Stream: Read + Write + Send + Sized + 'static {
    /// # Errors
    ///
    /// Fails when the connection cannot be shared with another thread.
    fn try_clone(&self) -> io::Result<Self>;

    fn close(&self);
}

pub trait Listener {
    type Stream: Stream;

    /// # Errors
    ///
    /// Fails when no connection could be accepted.
    fn accept(&self) -> io::Result<Self::Stream>;
}

impl Stream for UnixStream {
    fn try_clone(&self) -> io::Result<Self> {
        Self::try_clone(self)
    }

    fn close(&self) {
        if let Err(error) = self.shutdown(Shutdown::Both) {
            tracing::debug!(%error, "a runtime connection was already closed");
        }
    }
}

impl Listener for UnixListener {
    type Stream = UnixStream;

    fn accept(&self) -> io::Result<UnixStream> {
        Self::accept(self).map(|(stream, _)| stream)
    }
}

/// # Errors
///
/// Fails when the runner's folder cannot be made private or the socket cannot be bound.
pub fn bind_runtime_socket(root: &Path) -> io::Result<UnixListener> {
    ensure_private_dir(root)?;
    let socket = runtime_socket(root);
    if socket.exists() {
        fs::remove_file(&socket)?;
    }
    UnixListener::bind(socket)
}
