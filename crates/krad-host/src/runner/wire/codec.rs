use krad_domain::{CloudProcessId, CloudProcessRun, JournalOffset};
use std::io::{self, Read, Write};

use crate::mux::TerminalSize;

const MAX_TEXT_BYTES: u32 = 256 * 1024;
const MAX_LIST_ITEMS: u32 = 4096;
pub(super) const MAX_DATA_BYTES: u32 = 1024 * 1024;
const ABSENT: u8 = 0;
const PRESENT: u8 = 1;
const RUNNING: u8 = 0;
const ENDED: u8 = 1;

pub(super) fn write_u8(out: &mut impl Write, byte: u8) -> io::Result<()> {
    out.write_all(&[byte])
}

pub(super) fn write_len(out: &mut impl Write, len: usize) -> io::Result<()> {
    let len = u32::try_from(len).map_err(|_| malformed())?;
    out.write_all(&len.to_be_bytes())
}

pub(super) fn write_bytes(out: &mut impl Write, bytes: &[u8]) -> io::Result<()> {
    write_len(out, bytes.len())?;
    out.write_all(bytes)
}

pub(super) fn write_text(out: &mut impl Write, text: &str) -> io::Result<()> {
    write_bytes(out, text.as_bytes())
}

pub(super) fn write_offset(out: &mut impl Write, offset: JournalOffset) -> io::Result<()> {
    out.write_all(&offset.0.to_be_bytes())
}

pub(super) fn write_size(out: &mut impl Write, size: TerminalSize) -> io::Result<()> {
    out.write_all(&size.cols.to_be_bytes())?;
    out.write_all(&size.rows.to_be_bytes())
}

pub(super) fn write_code(out: &mut impl Write, code: Option<i32>) -> io::Result<()> {
    match code {
        None => write_u8(out, ABSENT),
        Some(code) => {
            write_u8(out, PRESENT)?;
            out.write_all(&code.to_be_bytes())
        }
    }
}

pub(super) fn write_run(out: &mut impl Write, run: CloudProcessRun) -> io::Result<()> {
    match run {
        CloudProcessRun::Running => write_u8(out, RUNNING),
        CloudProcessRun::Ended(code) => {
            write_u8(out, ENDED)?;
            write_code(out, code)
        }
    }
}

pub(super) fn write_optional_size(
    out: &mut impl Write,
    size: Option<TerminalSize>,
) -> io::Result<()> {
    match size {
        None => write_u8(out, ABSENT),
        Some(size) => {
            write_u8(out, PRESENT)?;
            write_size(out, size)
        }
    }
}

pub(super) fn read_u8(input: &mut impl Read) -> io::Result<u8> {
    let mut byte = [0];
    input.read_exact(&mut byte)?;
    Ok(byte[0])
}

fn read_u16(input: &mut impl Read) -> io::Result<u16> {
    let mut raw = [0; 2];
    input.read_exact(&mut raw)?;
    Ok(u16::from_be_bytes(raw))
}

fn read_u32(input: &mut impl Read) -> io::Result<u32> {
    let mut raw = [0; 4];
    input.read_exact(&mut raw)?;
    Ok(u32::from_be_bytes(raw))
}

pub(super) fn read_offset(input: &mut impl Read) -> io::Result<JournalOffset> {
    let mut raw = [0; 8];
    input.read_exact(&mut raw)?;
    Ok(JournalOffset(u64::from_be_bytes(raw)))
}

pub(super) fn read_size(input: &mut impl Read) -> io::Result<TerminalSize> {
    Ok(TerminalSize {
        cols: read_u16(input)?,
        rows: read_u16(input)?,
    })
}

pub(super) fn read_code(input: &mut impl Read) -> io::Result<Option<i32>> {
    match read_u8(input)? {
        ABSENT => Ok(None),
        PRESENT => {
            let mut raw = [0; 4];
            input.read_exact(&mut raw)?;
            Ok(Some(i32::from_be_bytes(raw)))
        }
        _ => Err(malformed()),
    }
}

pub(super) fn read_run(input: &mut impl Read) -> io::Result<CloudProcessRun> {
    match read_u8(input)? {
        RUNNING => Ok(CloudProcessRun::Running),
        ENDED => Ok(CloudProcessRun::Ended(read_code(input)?)),
        _ => Err(malformed()),
    }
}

pub(super) fn read_optional_size(input: &mut impl Read) -> io::Result<Option<TerminalSize>> {
    match read_u8(input)? {
        ABSENT => Ok(None),
        PRESENT => Ok(Some(read_size(input)?)),
        _ => Err(malformed()),
    }
}

pub(super) fn read_count(input: &mut impl Read) -> io::Result<u32> {
    let count = read_u32(input)?;
    if count > MAX_LIST_ITEMS {
        return Err(malformed());
    }
    Ok(count)
}

pub(super) fn read_bytes(input: &mut impl Read, limit: u32) -> io::Result<Vec<u8>> {
    let len = read_u32(input)?;
    if len > limit {
        return Err(malformed());
    }
    let mut bytes = vec![0; len as usize];
    input.read_exact(&mut bytes)?;
    Ok(bytes)
}

pub(super) fn read_text(input: &mut impl Read) -> io::Result<String> {
    String::from_utf8(read_bytes(input, MAX_TEXT_BYTES)?).map_err(|_| malformed())
}

pub(super) fn read_parsed<T: std::str::FromStr>(input: &mut impl Read) -> io::Result<T> {
    read_text(input)?.parse().map_err(|_| malformed())
}

pub(super) fn read_id(input: &mut impl Read) -> io::Result<CloudProcessId> {
    read_parsed(input)
}

pub(super) fn malformed() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "malformed cloud runner message")
}
