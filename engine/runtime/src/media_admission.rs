//! Bound decoded chat requests before preparation, independently of model planning.

use std::{
    future::Future,
    sync::{atomic::AtomicBool, Arc},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, TryAcquireError};

/// One decoded request matches the host's one serialized native chat session.
/// This limits concurrency; byte limits and live-memory checks remain separate.
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
}

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
