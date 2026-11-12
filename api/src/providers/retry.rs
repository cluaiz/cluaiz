//! When a request to a provider is sent again: a rate limit (after the wait the provider names),
//! a busy provider, a connection that did not open, or an answer that came back empty. Every
//! wait ends early when the person cancels.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::errors::Failure;

/// Tries after the first, for a rate limit or a busy provider
const MOST_TRIES: u32 = 2;
/// The longest wait for a rate limit
const LONGEST_WAIT: Duration = Duration::from_secs(20);
/// An answer with no text, thinking or call is asked for once more
pub(super) const EMPTY_ANSWER_TRIES: u32 = 1;

/// How long to wait before try `attempt` (1 for the first retry) after `failure`; None: no retry
pub(super) fn wait_after(
    failure: Failure,
    attempt: u32,
    retry_after: Option<Duration>,
) -> Option<Duration> {
    if attempt > MOST_TRIES {
        return None;
    }
    match failure {
        Failure::RateLimited => Some(
            retry_after
                .unwrap_or(Duration::from_secs(2u64 << attempt.min(4)))
                .min(LONGEST_WAIT),
        ),
        Failure::Busy => Some(Duration::from_secs(if attempt == 1 { 1 } else { 3 })),
        _ => None,
    }
}

/// A connection that did not open or timed out is tried once more
pub(super) fn wait_after_send_error(error: &reqwest::Error, attempt: u32) -> Option<Duration> {
    (attempt == 1 && (error.is_connect() || error.is_timeout())).then(|| Duration::from_secs(1))
}

/// The provider's `Retry-After` header in seconds
pub(super) fn retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let seconds: f64 = headers
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()?;
    (seconds.is_finite() && seconds >= 0.0).then(|| Duration::from_secs_f64(seconds))
}

/// The wait a provider names in its message ("Please try again in 8.27s", "in 450ms"), when it
/// sends no `Retry-After` header
pub(super) fn retry_after_text(body: &str) -> Option<Duration> {
    let lower = body.to_lowercase();
    let after = &lower[lower.find("try again in ")? + "try again in ".len()..];
    let number: String = after
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let value: f64 = number.parse().ok()?;
    let unit = after[number.len()..].trim_start();
    let seconds = if unit.starts_with("ms") {
        value / 1000.0
    } else if unit.starts_with('m') && !unit.starts_with("ms") {
        value * 60.0
    } else {
        value
    };
    (seconds.is_finite() && seconds >= 0.0).then(|| Duration::from_secs_f64(seconds))
}

/// Waits `wait`; false when the person cancelled meanwhile
pub(super) async fn pause(wait: Duration, cancel: Option<&AtomicBool>) -> bool {
    let end = tokio::time::Instant::now() + wait;
    while tokio::time::Instant::now() < end {
        if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(100).min(end - tokio::time::Instant::now())).await;
    }
    !cancel.is_some_and(|c| c.load(Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_follow_the_kind_and_the_providers_word() {
        assert_eq!(
            retry_after_text("Please try again in 8.27s. Need more tokens?"),
            Some(Duration::from_secs_f64(8.27))
        );
        assert_eq!(
            retry_after_text("try again in 450ms"),
            Some(Duration::from_millis(450))
        );
        assert_eq!(
            retry_after_text("Try again in 1m."),
            Some(Duration::from_secs(60))
        );
        assert_eq!(retry_after_text("slow down"), None);
        // A rate limit waits what the provider says, at most LONGEST_WAIT; a busy provider 1s then 3s
        assert_eq!(
            wait_after(Failure::RateLimited, 1, Some(Duration::from_secs(8))),
            Some(Duration::from_secs(8))
        );
        assert_eq!(
            wait_after(Failure::RateLimited, 1, Some(Duration::from_secs(90))),
            Some(LONGEST_WAIT)
        );
        assert_eq!(
            wait_after(Failure::Busy, 2, None),
            Some(Duration::from_secs(3))
        );
        assert_eq!(wait_after(Failure::Busy, 3, None), None);
        // Never for a quota, a refused key, a full context or anything else
        for failure in [
            Failure::Quota,
            Failure::Key,
            Failure::ContextFull,
            Failure::NoModel,
            Failure::Other,
        ] {
            assert_eq!(wait_after(failure, 1, Some(Duration::from_secs(1))), None);
        }
    }
}
