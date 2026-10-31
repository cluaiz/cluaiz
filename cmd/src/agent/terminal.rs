//! One terminal frontend over the existing Agent::turn and Tools implementations.

use super::{
    attachments::{self, Attachment, Ingress, Input, Paste},
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
    display.emit(Event::Permission(agent.tools.strict));
    let metadata = Metadata {
        title: format!("Cluaiz · {}", agent.session.model_id),
        workspace: cluaiz_api::cep::roots::display(agent.tools.root()),
        status: agent.status(),
        // The mascot whenever it fits, else the wordmark
        brand: cluaiz_terminal::mascot::ART,
        compact_brand: crate::assets::logos::logo_gallery::WORDMARK,
        brand_blink: cluaiz_terminal::mascot::ART_BLINK,
    };
    let root = agent.tools.root().to_owned();
    let media = agent.session.options.settings()?.media;
    let ui_closing = closing.clone();
    let control = display.clone();
    let ui = tokio::task::spawn_blocking(move || {
        screen(metadata, receiver, input, ui_closing, root, control, media)
    });
    if let Some(task) = first {
        process(
            &mut agent,
            &display,
            Input {
                text: task,
                attachments: Vec::new(),
            },
        )
        .await;
    }
    display.emit(Event::Ready(agent.status()));
    while keep_open && !closing.load(Ordering::SeqCst) {
        let Some(text) = requests.recv().await else {
            break;
        };
        if closing.load(Ordering::SeqCst) {
            break;
        }
        match if text.attachments.is_empty() {
            agent.command(text.text.trim())
        } else {
            None
        } {
            Some(true) => break,
            Some(false) => {}
            None => process(&mut agent, &display, text).await,
        }
        display.emit(Event::Ready(agent.status()));
        display.emit(Event::Permission(agent.tools.strict));
    }
    display.emit(Event::Finished);
    ui.await
        .map_err(|error| eyre!("Terminal task failed: {error}"))??;
    Ok(())
}

async fn process(agent: &mut Agent, display: &Display, input: Input) {
    display.begin_turn();
    cluaiz_core::GLOBAL_CANCEL_SIGNAL.store(false, Ordering::SeqCst);
    let labels = input
        .attachments
        .iter()
        .map(|file| format!("[{}]", file.label))
        .collect::<Vec<_>>()
        .join(" ");
    display.emit(Event::User(format!(
        "{}{}{}",
        input.text,
        if labels.is_empty() { "" } else { "\n" },
        labels
    )));
    let result = async {
        let parts = if input.attachments.is_empty() {
            Vec::new()
        } else {
            display.emit(Event::Phase("Preparing attachments…".into()));
            attachments::prepare(
                input.attachments,
                &mut agent.tools,
                agent.session.options.settings()?.media,
                &agent.history,
                &agent.session.model_path,
            )
            .await?
        };
        agent.turn_prepared(&input.text, parts).await
    }
    .await;
    if let Err(error) = result {
        display.emit(Event::Error(error.to_string()));
    }
}

fn screen(
    metadata: Metadata,
    receiver: mpsc::Receiver<Event>,
    input: tokio::sync::mpsc::Sender<Input>,
    closing: Arc<AtomicBool>,
    root: std::path::PathBuf,
    control: Display,
    media: cluaiz_core::media::MediaSettings,
) -> std::io::Result<()> {
    let result = screen_inner(metadata, receiver, input, &closing, root, &control, media);
    closing.store(true, Ordering::SeqCst);
    cluaiz_core::GLOBAL_CANCEL_SIGNAL.store(true, Ordering::SeqCst);
    result
}

fn screen_inner(
    metadata: Metadata,
    receiver: mpsc::Receiver<Event>,
    input: tokio::sync::mpsc::Sender<Input>,
    closing: &AtomicBool,
    root: std::path::PathBuf,
    control: &Display,
    media: cluaiz_core::media::MediaSettings,
) -> std::io::Result<()> {
    let mut screen = Screen::open()?;
    let mut state = State::new(App::new(metadata, Limits::default()));
    state
        .app
        .transcript
        .set_assistant_layout(crate::chat::markdown::display_lines);
    let mut input = Some(input);
    let mut pending = None;
    let mut dirty = true;
    let mut last_draft = String::new();
    let (completion_sender, completions) = mpsc::sync_channel(1);
    let mut completion_job: Option<std::thread::JoinHandle<()>> = None;
    let mut animation = std::time::Instant::now();
    let mut tick = 0_u64;
    let mut files: Vec<Attachment> = Vec::new();
    let (paste_sender, pastes) = mpsc::sync_channel(1);
    let mut paste_job: Option<std::thread::JoinHandle<()>> = None;
    let mut deferred_paste: Option<String> = None;
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
        if let Ok(result) = pastes.try_recv() {
            state.app.pasting = false;
            match result {
                Ok(Paste::Text(text)) => {
                    if state.app.approval_pending() || state.app.theme_picker_open() {
                        deferred_paste = Some(text);
                        state.app.notice =
                            Some("Paste kept for your draft; finish the open dialog first".into());
                    } else {
                        state.app.handle(cluaiz_terminal::event::Event::Paste(text));
                    }
                }
                Ok(Paste::Files(mut added)) => {
                    let bytes = files.iter().chain(&added).try_fold(0_u64, |sum, file| {
                        sum.checked_add(file.retained_bytes() as u64)
                    });
                    if bytes.is_none_or(|bytes| media.limits.check_decoded_bytes(bytes).is_err()) {
                        state.app.notice = Some("Pending attachments exceed the configured media memory limit; previous attachments were kept".into());
                        dirty = true;
                        continue;
                    }
                    state
                        .app
                        .attachments
                        .extend(added.iter().map(|file| file.label.clone()));
                    files.append(&mut added);
                    state.app.notice =
                        Some("Attachment added · click its chip or Alt+Backspace to remove".into());
                }
                Err(error) => state.app.notice = Some(format!("{error:#}")),
            }
            dirty = true;
        }
        if !state.app.approval_pending() && !state.app.theme_picker_open() {
            if let Some(text) = deferred_paste.take() {
                state.app.handle(cluaiz_terminal::event::Event::Paste(text));
                dirty = true;
            }
        }
        if animation.elapsed() >= crate::ui::spinner::TICK_INTERVAL {
            animation = std::time::Instant::now();
            tick = tick.wrapping_add(1);
            state.app.activity = (state.busy && !state.app.approval_pending())
                .then(|| crate::ui::spinner::frame(tick).to_owned());
            dirty |= state.app.transcript.advance_animation() || state.busy;
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
        let action = if let cluaiz_terminal::event::Event::Paste(text) = &event {
            if !state.app.approval_pending()
                && !state.app.theme_picker_open()
                && attachments::candidate(text)
            {
                start_paste(
                    Ingress::Text(text.clone()),
                    &root,
                    media,
                    &paste_sender,
                    &mut paste_job,
                    &mut state.app,
                )?;
                continue;
            }
            state.app.handle(event)
        } else {
            state.app.handle(event)
        };
        match action {
            Action::Clipboard => start_paste(
                Ingress::Clipboard,
                &root,
                media,
                &paste_sender,
                &mut paste_job,
                &mut state.app,
            )?,
            Action::RemoveAttachment(index) if index < files.len() => {
                files.remove(index);
                state.app.attachments.remove(index);
            }
            Action::Permissions if pending.is_none() => {
                pending = Some(Input {
                    text: if state.app.strict { "/ask" } else { "/strict" }.into(),
                    attachments: Vec::new(),
                });
            }
            Action::ThinkingMode if pending.is_none() => {
                pending = Some(Input {
                    text: "/think".into(),
                    attachments: Vec::new(),
                })
            }
            Action::Permissions | Action::ThinkingMode => {
                state.app.notice =
                    Some("A turn is already queued; change modes after it starts".into())
            }
            Action::Submit(text) if !closing.load(Ordering::SeqCst) => {
                if let Some(path) = text.strip_prefix("/attach ") {
                    start_paste(
                        Ingress::Text(path.to_owned()),
                        &root,
                        media,
                        &paste_sender,
                        &mut paste_job,
                        &mut state.app,
                    )?;
                    continue;
                }
                if pending.is_none() {
                    let command = text.starts_with('/');
                    pending = Some(Input {
                        text,
                        attachments: if command {
                            Vec::new()
                        } else {
                            std::mem::take(&mut files)
                        },
                    });
                    if !command {
                        state.app.attachments.clear();
                    }
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

fn start_paste(
    input: Ingress,
    root: &std::path::Path,
    media: cluaiz_core::media::MediaSettings,
    sender: &mpsc::SyncSender<Result<Paste>>,
    job: &mut Option<std::thread::JoinHandle<()>>,
    app: &mut App,
) -> std::io::Result<()> {
    if job.as_ref().is_some_and(|job| !job.is_finished()) {
        app.notice = Some("A paste is still being prepared; try again when it finishes".into());
        return Ok(());
    }
    if let Some(previous) = job.take() {
        let _ = previous.join();
    }
    let root = root.to_owned();
    let sender = sender.clone();
    *job = Some(
        std::thread::Builder::new()
            .name("cluaiz-paste".into())
            .spawn(move || {
                let _ = sender.try_send(attachments::ingest(input, &root, media));
            })?,
    );
    app.notice = Some("Preparing paste…".into());
    app.begin_paste();
    Ok(())
}
