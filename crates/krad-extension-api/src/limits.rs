use std::time::Duration;

pub const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_HEADER_LINE_BYTES: usize = 1024;
pub const MAX_LOG_LINE_BYTES: usize = 8 * 1024;
pub const MAX_PENDING_PROMPTS: usize = 8;
pub const MAX_TERMINALS_PER_EXTENSION: usize = 16;
pub const TOAST_INTERVAL: Duration = Duration::from_millis(500);
pub const MAX_VIEW_DEPTH: usize = 64;
pub const MAX_VIEW_NODES: usize = 20_000;
pub const MAX_TITLEBAR_ITEMS_PER_EXTENSION: usize = 4;
pub const MAX_TITLEBAR_TITLE_CHARS: usize = 120;
