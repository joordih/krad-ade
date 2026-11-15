use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::thread;
use std::time::Duration;

const INPUT_THREAD: &str = "krad-cloud-connect-input";
const CHUNK_BYTES: usize = 32 * 1024;
const CLIENT_GONE_GRACE: Duration = Duration::from_secs(5);

fn pass_on(input: &mut impl Read, output: &mut impl Write) -> io::Result<()> {
    let mut chunk = vec![0; CHUNK_BYTES];
    loop {
        let read = match input.read(&mut chunk) {
            Ok(0) => return output.flush(),
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        output.write_all(&chunk[..read])?;
        output.flush()?;
    }
}

/// # Errors
///
/// Fails when the runner's runtime socket cannot be reached or the stream fails.
pub fn bridge(
    socket: &Path,
    input: impl Read + Send + 'static,
    output: &mut impl Write,
) -> io::Result<()> {
    bridge_within(socket, input, output, CLIENT_GONE_GRACE)
}

fn bridge_within(
    socket: &Path,
    mut input: impl Read + Send + 'static,
    output: &mut impl Write,
    grace: Duration,
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
            thread::sleep(grace);
            sending.shutdown(Shutdown::Both).ok();
        })?;
    pass_on(&mut stream, output)
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

    #[test]
    fn the_bridge_ends_soon_after_its_client_leaves_even_if_the_runtime_stays() -> io::Result<()> {
        let dir = std::env::temp_dir().join(format!("krad-bridge-left-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let socket = dir.join("runtime.sock");
        let listener = UnixListener::bind(&socket)?;
        let (accepted, held) = std::sync::mpsc::channel();
        thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                accepted.send(stream).ok();
            }
        });
        let started = std::time::Instant::now();
        let mut output = Vec::new();
        bridge_within(
            &socket,
            io::Cursor::new(Vec::new()),
            &mut output,
            Duration::from_millis(100),
        )?;
        assert!(started.elapsed() < Duration::from_secs(5));
        drop(held);
        std::fs::remove_dir_all(dir)
    }
}
