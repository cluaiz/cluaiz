//! Request-scoped observations, independent of inference backend and transport.
//! Counts are supplied only by the operation doing the work; elapsed time is not a percentage.

use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingStage {
    Queued,
    ModelLoading,
    Reading,
    Decoding,
    ProjectorLoading,
    Encoding,
    Prefill,
    Generation,
    Request,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingState {
    Running,
    Completed,
    Reused,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct AttachmentRef {
    pub message_index: usize,
    pub part_index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_index: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessingEvent {
    pub stage: ProcessingStage,
    pub state: ProcessingState,
    pub elapsed_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment: Option<AttachmentRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work: Option<ProcessingWork>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessingWork {
    pub completed: u64,
    pub total: u64,
    pub unit: &'static str,
}

struct Shared {
    start: Instant,
    terminal: AtomicBool,
    terminal_event: Mutex<Option<ProcessingEvent>>,
    cancel: Arc<AtomicBool>,
    message_offset: AtomicUsize,
    origins: Mutex<BTreeMap<(usize, usize), AttachmentRef>>,
    send: Box<dyn Fn(ProcessingEvent) + Send + Sync>,
}

#[derive(Clone)]
pub struct ProcessingReporter(Arc<Shared>);

impl ProcessingReporter {
    pub fn new(send: impl Fn(ProcessingEvent) + Send + Sync + 'static) -> Self {
        Self(Arc::new(Shared {
            start: Instant::now(),
            terminal: AtomicBool::new(false),
            terminal_event: Mutex::new(None),
            cancel: Arc::new(AtomicBool::new(false)),
            message_offset: AtomicUsize::new(0),
            origins: Mutex::new(BTreeMap::new()),
            send: Box::new(send),
        }))
    }

    pub fn cancellation(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.0.cancel)
    }
    pub fn cancelled(&self) -> bool {
        self.0.cancel.load(Ordering::Acquire)
    }
    pub fn set_message_offset(&self, offset: usize) {
        self.0.message_offset.store(offset, Ordering::Release);
    }
    pub fn source_attachment(&self, message_index: usize, part_index: usize) -> AttachmentRef {
        AttachmentRef {
            message_index: message_index
                .saturating_sub(self.0.message_offset.load(Ordering::Acquire)),
            part_index,
            frame_index: None,
        }
    }

    pub fn emit(
        &self,
        stage: ProcessingStage,
        state: ProcessingState,
        attachment: Option<AttachmentRef>,
        work: Option<ProcessingWork>,
    ) {
        if !self.0.terminal.load(Ordering::Acquire) {
            self.send(stage, state, attachment, work);
        }
    }

    fn send(
        &self,
        stage: ProcessingStage,
        state: ProcessingState,
        attachment: Option<AttachmentRef>,
        work: Option<ProcessingWork>,
    ) {
        let work = work.filter(|w| w.total > 0 && w.completed <= w.total);
        // Observers must not unwind across an FFI callback or stop inference.
        let event = ProcessingEvent {
            stage,
            state,
            attachment,
            work,
            elapsed_ms: self.0.start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        };
        if stage == ProcessingStage::Request {
            if let Ok(mut terminal) = self.0.terminal_event.lock() {
                *terminal = Some(event.clone());
            }
        }
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (self.0.send)(event)));
    }

    pub fn finish(&self, state: ProcessingState) {
        if !self.0.terminal.swap(true, Ordering::AcqRel) {
            self.send(ProcessingStage::Request, state, None, None);
        }
    }
    /// Reliable terminal snapshot even if a bounded transport dropped intermediate observations.
    pub fn terminal_event(&self) -> Option<ProcessingEvent> {
        self.0
            .terminal_event
            .lock()
            .ok()
            .and_then(|event| event.clone())
    }

    /// Preserve the uploaded part's identity when video expands to timestamp text and frames.
    pub fn map_prepared(&self, message: usize, part: usize, origin: AttachmentRef) {
        if let Ok(mut map) = self.0.origins.lock() {
            map.insert((message, part), origin);
        }
    }

    pub fn attachment(&self, message_index: usize, part_index: usize) -> AttachmentRef {
        self.0
            .origins
            .lock()
            .ok()
            .and_then(|map| map.get(&(message_index, part_index)).copied())
            .unwrap_or(AttachmentRef {
                message_index,
                part_index,
                frame_index: None,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_processing_terminal_is_once_and_video_origin_survives_expansion() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let received = events.clone();
        let reporter = ProcessingReporter::new(move |e| received.lock().unwrap().push(e));
        let origin = AttachmentRef {
            message_index: 1,
            part_index: 2,
            frame_index: Some(3),
        };
        reporter.set_message_offset(1);
        assert_eq!(reporter.source_attachment(2, 2).message_index, 1);
        reporter.map_prepared(1, 9, origin);
        assert_eq!(reporter.attachment(1, 9), origin);
        reporter.emit(
            ProcessingStage::Encoding,
            ProcessingState::Running,
            Some(origin),
            None,
        );
        reporter.finish(ProcessingState::Failed);
        reporter.finish(ProcessingState::Completed);
        assert_eq!(
            reporter.terminal_event().unwrap().state,
            ProcessingState::Failed
        );
        reporter.emit(
            ProcessingStage::Generation,
            ProcessingState::Running,
            None,
            None,
        );
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[1].state, ProcessingState::Failed);
        assert!(events[0].elapsed_ms <= events[1].elapsed_ms);
    }
}
