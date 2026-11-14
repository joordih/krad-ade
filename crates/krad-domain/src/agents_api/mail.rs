#[must_use]
pub fn notice_text(cli: &str, count: usize) -> String {
    format!("You have {count} new message(s) from other agents. Run `{cli} check` to read them.")
}
