use std::fs::File;
use std::io::{self, Read};

const OS_RANDOM: &str = "/dev/urandom";

pub(super) fn fill(buffer: &mut [u8]) -> io::Result<()> {
    File::open(OS_RANDOM)?.read_exact(buffer)
}
