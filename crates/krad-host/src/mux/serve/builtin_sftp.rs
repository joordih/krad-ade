use super::super::channel::Channel;
use super::super::wire::Stream;
use crate::sftp;

const SERVED: i32 = 0;
const FAILED: i32 = 1;

pub(super) fn run(channel: &Channel) {
    if channel.accept(None).is_err() {
        return;
    }
    channel.end(Stream::Errors);
    tracing::debug!("serving files with the built-in SFTP server");
    let served = sftp::serve(channel.reader(Stream::Main), channel.writer(Stream::Main));
    let code = match served {
        Ok(()) => SERVED,
        Err(error) => {
            tracing::debug!(%error, "the built-in SFTP server stopped");
            FAILED
        }
    };
    channel.exit(Some(code)).ok();
    channel.close();
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::super::super::session::tests::pair;
    use super::super::super::wire::OpenRequest;
    use super::*;
    use crate::sftp::{join, Sftp};

    const WAIT: Duration = Duration::from_secs(10);

    #[test]
    fn files_are_served_over_a_channel_without_an_sftp_server(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let ((client, _), (_host, incoming)) = pair()?;
        let serving =
            std::thread::spawn(move || -> Result<(), std::sync::mpsc::RecvTimeoutError> {
                let request = incoming.recv_timeout(WAIT)?;
                run(&request.channel);
                Ok(())
            });
        let (channel, _) = client.open(OpenRequest::Sftp, WAIT)?;
        let sftp = Sftp::handshake(
            channel.reader(Stream::Main),
            channel.writer(Stream::Main),
            WAIT,
        )?;
        let dir = std::env::temp_dir().join(format!("krad-host-mux-sftp-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let file = join(&dir.to_string_lossy(), "hello.txt");
        sftp.write(&file, b"hello")?;
        assert_eq!(sftp.read(&file, u64::MAX)?, b"hello");
        channel.close();
        drop(sftp);
        serving
            .join()
            .map_err(|_| "the serving thread panicked")??;
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
