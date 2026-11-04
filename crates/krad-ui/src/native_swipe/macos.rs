use std::ptr::{null_mut, NonNull};

use block2::RcBlock;
use futures::channel::mpsc::{unbounded, UnboundedReceiver};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{NSEvent, NSEventMask, NSEventType};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SwipeDirection {
    Left,
    Right,
}

impl SwipeDirection {
    pub(crate) const fn is_forward(self) -> bool {
        matches!(self, Self::Left)
    }
}

pub(crate) struct SwipeWatch {
    pub(crate) swipes: UnboundedReceiver<SwipeDirection>,
    monitor: Retained<AnyObject>,
}

impl Drop for SwipeWatch {
    fn drop(&mut self) {
        // SAFETY: `monitor` is the object AppKit returned from
        // `addLocalMonitorForEventsMatchingMask:handler:` and it is removed once.
        unsafe { NSEvent::removeMonitor(&self.monitor) };
    }
}

fn direction_of(delta_x: f64) -> Option<SwipeDirection> {
    if delta_x > 0.0 {
        Some(SwipeDirection::Right)
    } else if delta_x < 0.0 {
        Some(SwipeDirection::Left)
    } else {
        None
    }
}

pub(crate) fn watch_swipes() -> Option<SwipeWatch> {
    let (sender, swipes) = unbounded();
    let handler = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: AppKit hands the monitor a live event for the duration of the call.
        let native = unsafe { event.as_ref() };
        if native.r#type() != NSEventType::Swipe {
            return event.as_ptr();
        }
        match direction_of(native.deltaX()) {
            Some(direction) if sender.unbounded_send(direction).is_ok() => null_mut(),
            _ => event.as_ptr(),
        }
    });
    // SAFETY: the handler returns either the event it was given or null, as the
    // monitor contract requires.
    let monitor = unsafe {
        NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::Swipe, &handler)
    }?;
    Some(SwipeWatch { swipes, monitor })
}
