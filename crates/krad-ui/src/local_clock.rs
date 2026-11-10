use chrono::{Local, Timelike};
use krad_domain::TimeOfDay;

pub(crate) fn now() -> Option<TimeOfDay> {
    let now = Local::now();
    TimeOfDay::new(
        u8::try_from(now.hour()).ok()?,
        u8::try_from(now.minute()).ok()?,
    )
    .ok()
}
