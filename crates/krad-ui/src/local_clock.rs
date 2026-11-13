use chrono::{Local, Offset as _, Timelike};
use krad_domain::{LocalMoment, LocalOffset, TimeOfDay};
use std::time::SystemTime;

#[must_use]
pub fn local_moment() -> LocalMoment {
    let now = Local::now();
    LocalMoment {
        at: SystemTime::from(now),
        offset: LocalOffset::east_seconds(now.offset().fix().local_minus_utc()).unwrap_or_default(),
    }
}

pub(crate) fn now() -> Option<TimeOfDay> {
    let now = Local::now();
    TimeOfDay::new(
        u8::try_from(now.hour()).ok()?,
        u8::try_from(now.minute()).ok()?,
    )
    .ok()
}
