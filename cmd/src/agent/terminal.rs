//! One terminal frontend over the existing Agent::turn and Tools implementations.

use super::{
    display::{Display, Event},
    terminal_state::State,
    Agent,
};
use cluaiz_terminal::{render, Action, App, Limits, Metadata, Screen};
use color_eyre::{eyre::eyre, Result};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::Duration,
};

const EVENT_CAPACITY: usize = 128;
const INPUT_CAPACITY: usize = 1;
const INPUT_POLL: Duration = Duration::from_millis(40);

pub async fn run(mut agent: Agent, first: Option<String>, keep_open: bool) -> Result<()> {
    let (events, receiver) = mpsc::sync_channel(EVENT_CAPACITY);
    let (input, mut requests) = tokio::sync::mpsc::channel(INPUT_CAPACITY);
    let closing = Arc::new(AtomicBool::new(false));
    let display = Display::screen(events, closing.clone());
    agent.display = display.clone();
    agent.tools.set_display(display.clone());
    let metadata = Metadata {
        title: format!("Cluaiz · {}", agent.session.model_id),
        workspace: cluaiz_api::cep::roots::display(agent.tools.root()),
        status: agent.status(),
        brand: crate::assets::logos::COG,
        compact_brand: crate::assets::logos::COG_COMPACT,
    };
    let root = agent.tools.root().to_owned();
    let ui_closing = closing.clone();
    let control = display.clone();
    let ui = tokio::task::spawn_blocking(move || {
        screen(metadata, receiver, input, ui_closing, root, control)
    });
    display.emit(Event::Message(
        "Welcome".into(),
        "What would you like to work on?\nType / for commands or @ for workspace files.".into(),
    ));
    if let Some(task) = first {
        process(&mut agent, &display, &task).await;
    }
    display.emit(Event::Ready(agent.status()));
    while keep_open && !closing.load(Ordering::SeqCst) {
        let Some(text) = requests.recv().await else {
            break;
        };
        if closing.load(Ordering::SeqCst) {
            break;
        }
        match agent.command(text.trim()) {
            Some(true) => break,
            Some(false) => {}
            None => process(&mut agent, &display, &text).await,
        }
        display.emit(Event::Ready(agent.status()));
    }
    display.emit(Event::Finished);
    ui.await
        .map_err(|error| eyre!("Terminal task failed: {error}"))??;
    Ok(())
}

async fn process(agent: &mut Agent, display: &Display, text: &str) {
    display.emit(Event::User(text.into()));
    if let Err(error) = agent.turn(text).await {
        display.emit(Event::Error(error.to_string()));
    }
}

fn screen(
    metadata: Metadata,
    receiver: mpsc::Receiver<Event>,
    input: tokio::sync::mpsc::Sender<String>,
    closing: Arc<AtomicBool>,
    root: std::path::PathBuf,
    control: Display,
) -> std::io::Result<()> {
    let result = screen_inner(metadata, receiver, input, &closing, root, &control);
    closing.store(true, Ordering::SeqCst);
    cluaiz_core::GLOBAL_CANCEL_SIGNAL.store(true, Ordering::SeqCst);
    result
}

fn screen_inner(
    metadata: Metadata,
    receiver: mpsc::Receiver<Event>,
    input: tokio::sync::mpsc::Sender<String>,
    closing: &AtomicBool,
    root: std::path::PathBuf,
    control: &Display,
) -> std::io::Result<()> {
    let mut screen = Screen::open()?;
    let mut state = State::new(App::new(metadata, Limits::default()));
    let mut input = Some(input);
    let mut pending = None;
    let mut dirty = true;
    let mut last_draft = String::new();
    let (completion_sender, completions) = mpsc::sync_channel(1);
    let mut completion_job: Option<std::thread::JoinHandle<()>> = None;
    loop {
        for _ in 0..EVENT_CAPACITY {
            match receiver.try_recv() {
                Ok(event) => {
                    state.apply(event);
                    dirty = true;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    state.finished = true;
                    break;
                }
            }
        }
        if state.finished {
            break;
        }
        if !state.busy && input.is_some() {
            if let Some(text) = pending.take() {
                if let Some(sender) = &input {
                    match sender.try_send(text) {
                        Ok(()) => {
                            state.busy = true;
                            state.app.notice = None;
                        }
                        Err(error) => {
                            pending = Some(error.into_inner());
                        }
                    }
                }
            }
        }
        // Drain the previous result before starting a new lookup. Otherwise a fast new lookup
        // could lose its result to a stale item occupying the single bounded slot.
        if let Ok((query, start, suggestions)) = completions.try_recv() {
            if query == state.app.draft()
                && !state.app.approval_pending()
                && !state.app.theme_picker_open()
            {
                state.app.set_suggestions(start, suggestions);
                dirty = true;
            }
        }
        let draft = state.app.draft();
        if draft != last_draft
            && !state.app.approval_pending()
            && !state.app.theme_picker_open()
            && completion_job.as_ref().is_none_or(|job| job.is_finished())
        {
            if let Some(job) = completion_job.take() {
                let _ = job.join();
            }
            let directory = root.clone();
            let query = draft.clone();
            let sender = completion_sender.clone();
            completion_job = Some(
                std::thread::Builder::new()
                    .name("cluaiz-completion".into())
                    .spawn(move || {
                        let (start, suggestions) =
                            super::completion::suggestions(&directory, &query);
                        let _ = sender.try_send((query, start, suggestions));
                    })?,
            );
            last_draft = draft;
        }
        if dirty {
            screen.draw(|frame| render(frame, &mut state.app))?;
            dirty = false;
        }
        let Some(event) = screen.read_event(INPUT_POLL)? else {
            continue;
        };
        dirty = true;
        match state.app.handle(event) {
            Action::Submit(text) if !closing.load(Ordering::SeqCst) => {
                if pending.is_none() {
                    pending = Some(text);
                    state.app.notice = Some("Queued for the agent".into());
                } else {
                    let _ = state.app.set_draft(&text);
                    state.app.notice =
                        Some("One message is already queued; your draft was kept".into());
                }
            }
            Action::Cancel => {
                control.interrupt();
                pending = None;
                state.app.notice =
                    Some("Cancellation requested; waiting for the current operation".into());
            }
            Action::Approval(response) => {
                if let Some(reply) = state.approval.take() {
                    let _ = reply.send(response);
                }
            }
            Action::Exit => {
                closing.store(true, Ordering::SeqCst);
                cluaiz_core::GLOBAL_CANCEL_SIGNAL.store(true, Ordering::SeqCst);
                pending = None;
                input.take();
                state.deny_pending();
                state.app.notice = Some("Stopping agent…".into());
            }
            _ => {}
        }
    }
    state.deny_pending();
    screen.close()
}
