//! Bound decoded chat requests before preparation, independently of model planning.

use std::{
    future::Future,
    sync::{atomic::AtomicBool, Arc},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, TryAcquireError};

/// One decoded request matches the host's one serialized native chat session.
/// This limits concurrency; byte limits and live-memory checks remain separate.
#[derive(Clone)]
pub struct MediaAdmission(Arc<Semaphore>);

impl Default for MediaAdmission {
    fn default() -> Self {
        Self(Arc::new(Semaphore::new(1)))
    }
}

impl MediaAdmission {
    /// Reject before decoding instead of retaining an unbounded prepared queue.
    pub fn try_acquire(&self) -> Result<MediaLease, TryAcquireError> {
        self.0.clone().try_acquire_owned().map(|p| MediaLease {
            _permit: Arc::new(p),
        })
    }

    /// Waits in line for the permit, at most `wait`; `None` when the wait ran out. Requests that
    /// arrive while one runs are then served in arrival order (tokio's semaphore is fair) instead
    /// of refused, as llama-server and Ollama queue them. Run 26 (2026-11-20, 4 GB VPS): with
    /// `try_acquire`, a second concurrent embedding request got HTTP 429, so 2 parallel callers
    /// failed 99.96% of their requests. A waiting request holds only its undecoded input (text, or
    /// attachment bytes within the media byte limit); decoding starts after the permit, so decoded
    /// buffers never queue up.
    pub async fn acquire(&self, wait: std::time::Duration) -> Option<MediaLease> {
        let permit = tokio::time::timeout(wait, self.0.clone().acquire_owned()).await.ok()?.ok()?;
        Some(MediaLease { _permit: Arc::new(permit) })
    }
}

/// How long an embedding or speech request waits for the one running before it is refused
pub const QUEUE_WAIT: std::time::Duration = std::time::Duration::from_secs(120);

/// Clones share one permit, including tool continuations and native workers.
#[derive(Clone, Debug)]
pub struct MediaLease {
    _permit: Arc<OwnedSemaphorePermit>,
}

impl MediaLease {
    /// Dropping the caller cannot release admission while its decoder still runs.
    /// The completed result retains admission until its buffers are consumed/dropped.
    pub async fn prepare<F>(
        self,
        cancellation: Arc<AtomicBool>,
        work: F,
    ) -> Result<(F::Output, Self), tokio::task::JoinError>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let mut guard = cluaiz_media::request::PreparationGuard::new(cancellation);
        let result = tokio::spawn(async move { (work.await, self) }).await;
        guard.complete();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_admission_keeps_permit_until_last_request_owner_drops() {
        let admission = MediaAdmission::default();
        let lease = admission.try_acquire().unwrap();
        let worker = lease.clone();
        drop(lease);
        assert!(admission.try_acquire().is_err());
        drop(worker);
        assert!(admission.try_acquire().is_ok());
    }

    #[tokio::test]
    async fn a_second_request_waits_for_the_first_instead_of_failing() {
        let admission = MediaAdmission::default();
        let first = admission.try_acquire().unwrap();
        let waiting = admission.clone();
        let second = tokio::spawn(async move { waiting.acquire(std::time::Duration::from_secs(5)).await.is_some() });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(!second.is_finished(), "the second request waits while the first runs");
        drop(first);
        assert!(second.await.unwrap(), "and runs once the first is done");
        let held = admission.try_acquire().unwrap();
        assert!(admission.acquire(std::time::Duration::from_millis(20)).await.is_none(), "a wait that runs out is refused");
        drop(held);
    }

    #[tokio::test]
    async fn test_disconnected_preparation_keeps_permit_until_worker_finishes() {
        let admission = MediaAdmission::default();
        let lease = admission.try_acquire().unwrap();
        let cancellation = Arc::new(AtomicBool::new(false));
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (finish_tx, finish_rx) = tokio::sync::oneshot::channel();
        let caller = tokio::spawn(lease.prepare(cancellation.clone(), async move {
            started_tx.send(()).unwrap();
            finish_rx.await.unwrap();
        }));
        started_rx.await.unwrap();
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        assert!(cancellation.load(std::sync::atomic::Ordering::Acquire));
        assert!(admission.try_acquire().is_err());
        finish_tx.send(()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if admission.try_acquire().is_ok() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_preparation_result_holds_admission_and_error_releases_it() {
        let admission = MediaAdmission::default();
        let cancellation = Arc::new(AtomicBool::new(false));
        let result = admission
            .try_acquire()
            .unwrap()
            .prepare(cancellation.clone(), async {
                Err::<(), _>("invalid source")
            })
            .await
            .unwrap();
        assert!(result.0.is_err());
        assert!(!cancellation.load(std::sync::atomic::Ordering::Acquire));
        assert!(admission.try_acquire().is_err());
        drop(result);
        assert!(admission.try_acquire().is_ok());
    }
}
