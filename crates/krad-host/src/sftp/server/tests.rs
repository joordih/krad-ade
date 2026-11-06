use std::fs;
use std::io::{self, Write};
use std::net::Shutdown;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::thread::{self, JoinHandle};
use std::time::{Duration, UNIX_EPOCH};

use super::super::wire::{
    Attrs, Message, OpenFlags, Packet, PacketType, StatusCode, MAX_PACKET_BYTES, PROTOCOL_VERSION,
};
use super::super::POSIX_RENAME_EXTENSION;
use super::serve;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const UNKNOWN_TYPE_FRAME: [u8; 9] = [0, 0, 0, 5, 99, 0, 0, 0, 7];

pub(super) fn scratch(name: &str) -> io::Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("krad-host-builtin-{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;
    dir.canonicalize()
}

fn bytes(path: &Path) -> &[u8] {
    path.as_os_str().as_encoded_bytes()
}

fn init_frame() -> io::Result<Vec<u8>> {
    let mut frame = Vec::new();
    Packet::new(PacketType::Init)
        .u32(PROTOCOL_VERSION)
        .send(&mut frame)?;
    Ok(frame)
}

struct Peer {
    stream: UnixStream,
    next_id: u32,
    server: JoinHandle<io::Result<()>>,
}

impl Peer {
    fn start() -> io::Result<(Self, Message)> {
        let (client, host) = UnixStream::pair()?;
        let reader = host.try_clone()?;
        let server = thread::spawn(move || serve(reader, host));
        let mut peer = Self {
            stream: client,
            next_id: 1,
            server,
        };
        peer.stream.write_all(&init_frame()?)?;
        let version = Message::receive(&mut peer.stream)?;
        Ok((peer, version))
    }

    fn call(
        &mut self,
        kind: PacketType,
        fill: impl FnOnce(&mut Packet) -> io::Result<()>,
    ) -> io::Result<Message> {
        let id = self.next_id;
        self.next_id += 1;
        let mut packet = Packet::request(kind, id);
        fill(&mut packet)?;
        packet.send(&mut self.stream)?;
        let reply = Message::receive(&mut self.stream)?;
        assert_eq!(reply.id, id);
        Ok(reply)
    }

    fn path_call(&mut self, kind: PacketType, path: &Path) -> io::Result<Message> {
        self.call(kind, |packet| packet.bytes(bytes(path)).map(drop))
    }

    fn open(&mut self, path: &Path, flags: OpenFlags, mode: Option<u32>) -> io::Result<Vec<u8>> {
        handle(self.call(PacketType::Open, |packet| {
            packet.bytes(bytes(path))?.u32(flags.0).mode_attrs(mode);
            Ok(())
        })?)
    }

    fn finish(self) -> TestResult {
        self.stream.shutdown(Shutdown::Write)?;
        self.server.join().map_err(|_| "the server panicked")??;
        Ok(())
    }
}

fn status(mut reply: Message) -> io::Result<StatusCode> {
    assert_eq!(reply.kind, PacketType::Status);
    Ok(StatusCode::from(reply.u32()?))
}

fn handle(mut reply: Message) -> io::Result<Vec<u8>> {
    assert_eq!(reply.kind, PacketType::Handle);
    reply.bytes()
}

fn attrs(mut reply: Message) -> io::Result<Attrs> {
    assert_eq!(reply.kind, PacketType::Attrs);
    reply.attrs()
}

#[test]
fn the_handshake_offers_protocol_three_with_posix_rename() -> TestResult {
    let (peer, mut version) = Peer::start()?;
    assert_eq!(version.kind, PacketType::Version);
    assert_eq!(version.u32()?, PROTOCOL_VERSION);
    assert_eq!(version.string()?, POSIX_RENAME_EXTENSION);
    version.bytes()?;
    assert!(!version.has_more());
    peer.finish()
}

#[test]
fn a_session_must_open_with_init_and_ends_when_the_client_leaves() -> TestResult {
    let mut stat = Vec::new();
    Packet::request(PacketType::Stat, 1)
        .str("/")?
        .send(&mut stat)?;
    assert!(serve(stat.as_slice(), Vec::new()).is_err());
    let mut out = Vec::new();
    serve(init_frame()?.as_slice(), &mut out)?;
    assert_eq!(
        Message::receive(&mut out.as_slice())?.kind,
        PacketType::Version
    );
    Ok(())
}

#[test]
fn unknown_and_malformed_requests_are_answered_without_ending_the_session() -> TestResult {
    let (mut peer, _) = Peer::start()?;
    peer.stream.write_all(&UNKNOWN_TYPE_FRAME)?;
    let unknown = Message::receive(&mut peer.stream)?;
    assert_eq!(unknown.id, 7);
    assert_eq!(status(unknown)?, StatusCode::OpUnsupported);
    let extension = peer.call(PacketType::Extended, |packet| {
        packet.str("statvfs@openssh.com")?.str("/")?;
        Ok(())
    })?;
    assert_eq!(status(extension)?, StatusCode::OpUnsupported);
    assert_eq!(
        status(peer.call(PacketType::Stat, |_| Ok(()))?)?,
        StatusCode::BadMessage
    );
    let unknown_handle = peer.call(PacketType::Close, |packet| packet.bytes(&[9]).map(drop))?;
    assert_eq!(status(unknown_handle)?, StatusCode::Failure);
    assert_eq!(
        attrs(peer.path_call(PacketType::Stat, Path::new("/"))?)?.kind(),
        crate::FileKind::Directory
    );
    peer.finish()
}

#[test]
fn missing_paths_answer_no_such_file() -> TestResult {
    let (mut peer, _) = Peer::start()?;
    let missing = scratch("missing")?.join("nothing-here");
    for kind in [
        PacketType::Stat,
        PacketType::Lstat,
        PacketType::Opendir,
        PacketType::Remove,
        PacketType::Rmdir,
        PacketType::Realpath,
        PacketType::Readlink,
    ] {
        assert_eq!(
            status(peer.path_call(kind, &missing)?)?,
            StatusCode::NoSuchFile
        );
    }
    let opened = peer.call(PacketType::Open, |packet| {
        packet
            .bytes(bytes(&missing))?
            .u32(OpenFlags::READ.0)
            .empty_attrs();
        Ok(())
    })?;
    assert_eq!(status(opened)?, StatusCode::NoSuchFile);
    peer.finish()
}

#[test]
fn reads_are_bounded_and_end_with_eof() -> TestResult {
    let dir = scratch("reads")?;
    let file = dir.join("big.bin");
    let body = vec![7_u8; 300 * 1024];
    fs::write(&file, &body)?;
    let (mut peer, _) = Peer::start()?;
    let opened = peer.open(&file, OpenFlags::READ, None)?;
    let asked = u32::try_from(body.len())?;
    let mut reply = peer.call(PacketType::Read, |packet| {
        packet.bytes(&opened)?.u64(0).u32(asked);
        Ok(())
    })?;
    assert_eq!(reply.kind, PacketType::Data);
    let data = reply.bytes()?;
    assert_ne!(data.len(), 0);
    assert!(data.len() < body.len());
    assert!(u32::try_from(data.len())? < MAX_PACKET_BYTES);
    let past_end = peer.call(PacketType::Read, |packet| {
        packet.bytes(&opened)?.u64(u64::from(asked)).u32(asked);
        Ok(())
    })?;
    assert_eq!(status(past_end)?, StatusCode::Eof);
    let closed = peer.call(PacketType::Close, |packet| packet.bytes(&opened).map(drop))?;
    assert_eq!(status(closed)?, StatusCode::Ok);
    peer.finish()?;
    fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn handles_carry_stat_and_permission_changes() -> TestResult {
    let dir = scratch("handles")?;
    let file = dir.join("token");
    let (mut peer, _) = Peer::start()?;
    let opened = peer.open(&file, OpenFlags::WRITE_NEW, Some(0o600))?;
    let written = peer.call(PacketType::Write, |packet| {
        packet.bytes(&opened)?.u64(0).bytes(b"secret")?;
        Ok(())
    })?;
    assert_eq!(status(written)?, StatusCode::Ok);
    let changed = peer.call(PacketType::Fsetstat, |packet| {
        packet.bytes(&opened)?.mode_attrs(Some(0o640));
        Ok(())
    })?;
    assert_eq!(status(changed)?, StatusCode::Ok);
    let seen = attrs(peer.call(PacketType::Fstat, |packet| packet.bytes(&opened).map(drop))?)?;
    assert_eq!(seen.size, Some(6));
    assert_eq!(seen.mode(), Some(0o640));
    assert!(seen.owner.is_some());
    assert!(seen.modified_secs.is_some());
    assert_eq!(fs::metadata(&file)?.permissions().mode() & 0o777, 0o640);
    peer.finish()?;
    fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn setstat_truncates_and_sets_times() -> TestResult {
    let dir = scratch("setstat")?;
    let file = dir.join("notes.txt");
    fs::write(&file, "hello world")?;
    let (mut peer, _) = Peer::start()?;
    let wanted = Attrs {
        size: Some(5),
        accessed_secs: Some(1_600_000_000),
        modified_secs: Some(1_700_000_000),
        ..Attrs::default()
    };
    let changed = peer.call(PacketType::Setstat, |packet| {
        packet.bytes(bytes(&file))?.attrs(&wanted);
        Ok(())
    })?;
    assert_eq!(status(changed)?, StatusCode::Ok);
    assert_eq!(fs::read(&file)?, b"hello");
    assert_eq!(
        fs::metadata(&file)?.modified()?,
        UNIX_EPOCH + Duration::from_secs(1_700_000_000)
    );
    peer.finish()?;
    fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn listing_a_large_folder_spans_several_replies_and_ends_with_eof() -> TestResult {
    let dir = scratch("listing")?;
    for index in 0..250 {
        fs::write(dir.join(format!("{index}.txt")), "")?;
    }
    let (mut peer, _) = Peer::start()?;
    let opened = handle(peer.path_call(PacketType::Opendir, &dir)?)?;
    let mut names = Vec::new();
    let mut replies = 0;
    loop {
        let mut reply = peer.call(PacketType::Readdir, |packet| {
            packet.bytes(&opened).map(drop)
        })?;
        if reply.kind == PacketType::Status {
            assert_eq!(status(reply)?, StatusCode::Eof);
            break;
        }
        replies += 1;
        for _ in 0..reply.u32()? {
            names.push(reply.string()?);
            reply.string()?;
            assert_eq!(reply.attrs()?.size, Some(0));
        }
    }
    assert!(replies > 1);
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 250);
    peer.finish()?;
    fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn plain_rename_keeps_a_taken_name_while_posix_rename_replaces_it() -> TestResult {
    let dir = scratch("renames")?;
    let (from, to) = (dir.join("from"), dir.join("to"));
    fs::write(&from, "new")?;
    fs::write(&to, "old")?;
    let (mut peer, _) = Peer::start()?;
    let plain = peer.call(PacketType::Rename, |packet| {
        packet.bytes(bytes(&from))?.bytes(bytes(&to))?;
        Ok(())
    })?;
    assert_eq!(status(plain)?, StatusCode::Failure);
    assert_eq!(fs::read(&to)?, b"old");
    let posix = peer.call(PacketType::Extended, |packet| {
        packet
            .str(POSIX_RENAME_EXTENSION)?
            .bytes(bytes(&from))?
            .bytes(bytes(&to))?;
        Ok(())
    })?;
    assert_eq!(status(posix)?, StatusCode::Ok);
    assert_eq!(fs::read(&to)?, b"new");
    assert!(!from.exists());
    peer.finish()?;
    fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn links_are_read_back_and_paths_resolved() -> TestResult {
    let dir = scratch("readlink")?;
    let target = dir.join("target.txt");
    fs::write(&target, "x")?;
    let link = dir.join("link.txt");
    std::os::unix::fs::symlink(&target, &link)?;
    let (mut peer, _) = Peer::start()?;
    let mut link_target = peer.path_call(PacketType::Readlink, &link)?;
    assert_eq!(link_target.kind, PacketType::Name);
    assert_eq!(link_target.u32()?, 1);
    assert_eq!(link_target.bytes()?, bytes(&target));
    let mut real = peer.path_call(PacketType::Realpath, &link)?;
    assert_eq!(real.u32()?, 1);
    assert_eq!(real.bytes()?, bytes(&target));
    let mut current = peer.path_call(PacketType::Realpath, Path::new(""))?;
    assert_eq!(current.u32()?, 1);
    assert!(current.string()?.starts_with('/'));
    peer.finish()?;
    fs::remove_dir_all(dir)?;
    Ok(())
}
