use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::thread;

const INPUT_THREAD: &str = "krad-cloud-connect-input";

/// # Errors
///
/// Fails when the runner's runtime socket cannot be reached or the stream fails.
pub fn bridge(
    socket: &Path,
    mut input: impl Read + Send + 'static,
    output: &mut impl Write,
) -> io::Result<()> {
    let mut stream = UnixStream::connect(socket)?;
    let mut sending = stream.try_clone()?;
    thread::Builder::new()
        .name(INPUT_THREAD.to_owned())
        .spawn(move || {
            if let Err(error) = io::copy(&mut input, &mut sending) {
                tracing::debug!(%error, "the bridge to the runner's runtime stopped sending");
            }
            sending.shutdown(Shutdown::Write).ok();
        })?;
    io::copy(&mut stream, output)?;
    output.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    #[test]
    fn the_bridge_carries_bytes_both_ways_until_the_runtime_closes() -> io::Result<()> {
        let dir = std::env::temp_dir().join(format!("krad-bridge-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let socket = dir.join("runtime.sock");
        let listener = UnixListener::bind(&socket)?;
        let echo = thread::spawn(move || -> io::Result<()> {
            let (mut stream, _) = listener.accept()?;
            let mut heard = Vec::new();
            stream.read_to_end(&mut heard)?;
            stream.write_all(&heard)
        });
        let mut output = Vec::new();
        bridge(&socket, io::Cursor::new(b"hello".to_vec()), &mut output)?;
        echo.join()
            .map_err(|_| io::Error::other("the echo panicked"))??;
        assert_eq!(output, b"hello");
        std::fs::remove_dir_all(dir)
    }
}
