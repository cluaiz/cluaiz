//! Exclusive screen ownership and restoration, including unwinding failures.

use std::io::{self, IsTerminal, Stdout};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Once,
};

use crossterm::{
    cursor::Show,
    event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, is_raw_mode_enabled, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use ratatui::{backend::CrosstermBackend, Frame, Terminal};

static ACTIVE: AtomicBool = AtomicBool::new(false);
static OWNED: AtomicBool = AtomicBool::new(false);
static PANIC_HOOK: Once = Once::new();

/// Returns false for redirected input/output and explicitly dumb terminals.
pub fn is_interactive() -> bool {
    io::stdin().is_terminal()
        && io::stdout().is_terminal()
        && std::env::var_os("TERM").is_none_or(|value| value != "dumb")
}

/// A single terminal screen. Drop restores modes; close also reports cleanup errors.
pub struct Screen {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    _mode: Mode,
}

impl Screen {
    /// Takes the terminal only when both input and output support interaction.
    pub fn open() -> io::Result<Self> {
        if !is_interactive() {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Interactive terminal required; use plain output",
            ));
        }
        let mode = Mode::enter()?;
        let terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
        Ok(Self {
            terminal,
            _mode: mode,
        })
    }

    /// Draws through Ratatui's buffer diff; no worker may write to this screen directly.
    pub fn draw(&mut self, draw: impl FnOnce(&mut Frame)) -> io::Result<()> {
        self.terminal.draw(draw).map(|_| ())
    }

    /// Reads from the same Crossterm backend used by this screen, with a bounded wait.
    pub fn read_event(
        &self,
        timeout: std::time::Duration,
    ) -> io::Result<Option<crossterm::event::Event>> {
        if crossterm::event::poll(timeout)? {
            crossterm::event::read().map(Some)
        } else {
            Ok(None)
        }
    }

    /// Hands the screen to `f` (the mascot playground draws anything it likes), then repaints the
    /// caller's view from scratch.
    pub fn takeover<T>(&mut self, f: impl FnOnce(&mut Stdout) -> io::Result<T>) -> io::Result<T> {
        let result = f(&mut io::stdout());
        self.terminal.clear()?;
        result
    }

    /// Restores the original screen and input behavior before returning to the caller.
    pub fn close(mut self) -> io::Result<()> {
        self._mode.close()
    }
}

struct Mode {
    armed: bool,
}

impl Mode {
    fn enter() -> io::Result<Self> {
        if is_raw_mode_enabled()? {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Raw terminal input is already owned by another caller",
            ));
        }
        OWNED
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "A terminal screen is already active",
                )
            })?;
        let mode = Self { armed: true };
        ACTIVE.store(true, Ordering::SeqCst);
        PANIC_HOOK.call_once(|| {
            let previous = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                let _ = restore();
                previous(info);
            }));
        });
        enable_raw_mode()?;
        execute!(
            io::stdout(),
            EnterAlternateScreen,
            EnableBracketedPaste,
            EnableMouseCapture
        )?;
        Ok(mode)
    }

    fn close(&mut self) -> io::Result<()> {
        if !self.armed {
            return Ok(());
        }
        self.armed = false;
        let result = restore();
        OWNED.store(false, Ordering::SeqCst);
        result
    }
}

impl Drop for Mode {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

fn restore() -> io::Result<()> {
    if !ACTIVE.swap(false, Ordering::SeqCst) {
        return Ok(());
    }
    // Attempt both cleanups even if one fails.
    let screen = execute!(
        io::stdout(),
        DisableBracketedPaste,
        DisableMouseCapture,
        LeaveAlternateScreen,
        Show
    );
    let raw = disable_raw_mode();
    screen.and(raw)
}
