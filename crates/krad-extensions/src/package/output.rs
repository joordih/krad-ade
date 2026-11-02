use std::process::Output;

use crate::error::ExtensionError;

const KEPT_OUTPUT_BYTES: usize = 4 * 1024;

pub(crate) fn tail(bytes: &[u8]) -> String {
    let start = bytes.len().saturating_sub(KEPT_OUTPUT_BYTES);
    String::from_utf8_lossy(&bytes[start..]).trim().to_owned()
}

pub(crate) fn checked(program: &str, output: &Output) -> Result<(), ExtensionError> {
    if output.status.success() {
        return Ok(());
    }
    let mut combined = output.stderr.clone();
    if combined.is_empty() {
        combined.clone_from(&output.stdout);
    }
    let failed = ExtensionError::ToolFailed {
        program: program.to_owned(),
        status: output.status.code(),
        output: tail(&combined),
    };
    tracing::warn!(program, status = ?output.status.code(), "install step failed");
    Err(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_end_of_long_output_is_kept() {
        let long = vec![b'x'; KEPT_OUTPUT_BYTES * 3];
        assert_eq!(tail(&long).len(), KEPT_OUTPUT_BYTES);
        assert_eq!(tail(b"  npm ERR! 404 \n"), "npm ERR! 404");
    }
}
