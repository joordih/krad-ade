use super::write_response;
use crate::feed::{AgentFeed, Subscription};
use crate::protocol::{AgentSubscription, ApiError, ErrorCode, Response, Timestamp};
use std::net::TcpStream;
use std::sync::atomic::Ordering;
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
const SLOW_SUBSCRIBER: &str =
    "The subscriber fell too far behind the agent events and was dropped; subscribe again with since_seq";
const FEED_STOPPED: &str = "The app stopped publishing agent events";

pub(crate) fn heartbeat_wait(last_sent: Instant, now: Instant) -> Duration {
    HEARTBEAT_INTERVAL.saturating_sub(now.saturating_duration_since(last_sent))
}

pub(crate) fn ending(lagged: bool) -> ApiError {
    if lagged {
        ApiError::new(ErrorCode::SlowSubscriber, SLOW_SUBSCRIBER)
    } else {
        ApiError::new(ErrorCode::Unavailable, FEED_STOPPED)
    }
}

pub(super) fn stream_agents(stream: &TcpStream, feed: &AgentFeed, subscription: AgentSubscription) {
    if stream.set_write_timeout(Some(WRITE_TIMEOUT)).is_err() {
        return;
    }
    let subscription = feed.subscribe(subscription, Timestamp::now());
    let id = subscription.id;
    send_lines(stream, feed, subscription);
    feed.unsubscribe(id);
}

fn send_lines(stream: &TcpStream, feed: &AgentFeed, subscription: Subscription) {
    let Subscription {
        opening,
        lines,
        lagged,
        ..
    } = subscription;
    if opening
        .iter()
        .any(|line| write_response(stream, line).is_err())
    {
        return;
    }
    let mut last_sent = Instant::now();
    loop {
        let line = match lines.recv_timeout(heartbeat_wait(last_sent, Instant::now())) {
            Ok(line) => line,
            Err(RecvTimeoutError::Timeout) => Response::Heartbeat(feed.heartbeat(Timestamp::now())),
            Err(RecvTimeoutError::Disconnected) => {
                let error = ending(lagged.load(Ordering::Acquire));
                let _told = write_response(stream, &Response::Error(error));
                return;
            }
        };
        if write_response(stream, &line).is_err() {
            return;
        }
        last_sent = Instant::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A_BIT: Duration = Duration::from_secs(4);

    #[test]
    fn a_heartbeat_follows_the_last_line_after_the_interval() {
        let sent = Instant::now();
        assert_eq!(heartbeat_wait(sent, sent), HEARTBEAT_INTERVAL);
        assert_eq!(
            heartbeat_wait(sent, sent + A_BIT) + A_BIT,
            HEARTBEAT_INTERVAL
        );
        assert_eq!(
            heartbeat_wait(sent, sent + HEARTBEAT_INTERVAL),
            Duration::ZERO
        );
        assert_eq!(
            heartbeat_wait(sent, sent + HEARTBEAT_INTERVAL + A_BIT),
            Duration::ZERO
        );
        assert_eq!(heartbeat_wait(sent + A_BIT, sent), HEARTBEAT_INTERVAL);
    }

    #[test]
    fn a_dropped_subscriber_learns_why() {
        assert_eq!(ending(true).code, ErrorCode::SlowSubscriber);
        assert_eq!(ending(false).code, ErrorCode::Unavailable);
    }
}
