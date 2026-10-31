# Component: terminal presentation

## Technical specification

- **Purpose:** Presentation-only Rust library for an interactive client. It owns screen drawing,
  input editing, theme preview and a bounded transcript projection. Execution stays in the caller.
- **Client:** The existing `cluaiz agent` command supplies real generation/tool events through
  `cmd/src/agent/terminal.rs`. This library remains presentation-only; it does not execute tools.
  Saved sessions and the remaining shared-controller migration are separate work.
- **Dependencies:** Ratatui, its maintained textarea widget, Crossterm and textwrap. No workspace
  dependency, model backend, GPU driver, network client or filesystem service.
- **Platform boundary:** Cross-platform terminal APIs. Automated backend tests do not establish
  native keyboard/font behavior on every OS. The ignored preview requires a real interactive TTY.

## Architectural flow

```mermaid
flowchart LR
    Caller["Client supplies metadata and display entries"] --> State["App and bounded transcript"]
    Input["Crossterm key / paste / resize"] --> Reducer["App.handle"]
    Reducer --> State
    Reducer --> Action["Submit / Cancel / Approval / Exit returned to client"]
    State --> Render["Pure Ratatui rendering"]
    Render --> Screen["Exclusive Screen guard"]
    Screen --> Restore["Normal close, error or panic restores terminal modes"]
```

## API contract

- `App::new(Metadata, Limits)` initializes display state. Metadata is supplied by the client;
  there is no model discovery or hidden settings/home-directory lookup.
- `App::handle(Event) -> Action` handles UI input. The client must process Submit/Cancel/Exit;
  receiving a Submit action does not mean anything was executed or persisted.
- `App::request_approval` preserves the user's draft. Responses allow once, request a tool-scoped
  session grant, deny, or return case-preserved instructions. The client owns authorization. Long
  operations scroll with PgUp/PgDn; allowing is blocked until their final line is visible. Esc denies.
- `App::set_suggestions` accepts client-provided command/path choices. This library does no path
  enumeration. Tab inserts, Enter chooses; file choices do not submit a message. Completion is
  suppressed while editing in the middle of a draft so a suffix cannot be accidentally replaced.
- `Transcript::upsert(id, label, content)` inserts or replaces a stable display entry. Oversized
  entries return an error instead of silently shortening content. The caller supplies a preview
  and a full-output mechanism when needed. Exceeding entry retention evicts older display entries
  and exposes an omitted count; this is not a durable-history store.
- `Transcript::upsert_role` accepts an explicit user/assistant/thinking/tool/error/notice role.
  Markers retain meaning in No color mode; current theme styles are applied to cached rows. Thinking
  starts collapsed and Ctrl+O toggles its retained source without adding it to the user's draft.
- Metadata may supply original and compact brand art. The renderer uses a complete fitting variant
  beside header details, or text alone in a tiny viewport. Artwork stays owned by the caller.
- `render(Frame, &mut App)` also works with Ratatui's TestBackend without opening a real screen.
- `Screen::open/draw/close` owns the real alternate screen. Redirected or dumb terminals return
  Unsupported; the caller must choose plain output. A second active screen is rejected.
- `Screen` installs one process-wide panic wrapper that restores an active screen before delegating
  to the previously installed panic hook. It cannot recover modes after a forced OS process kill.
- The client owns its worker/event loop, logging handoff and persistence. Workers must not print
  over the screen. This library has no spinner thread, execution policy or agent-event serializer.

## Files and ownership

| File | Responsibility |
| --- | --- |
| `src/terminal.rs` | TTY detection, exclusive ownership, raw mode, bracketed paste and restoration. |
| `src/app.rs` | UI-only reducer, theme-picker focus, submit/cancel/exit actions. |
| `src/input.rs` | Maintained textarea adapter, bounded UTF-8 draft, newline and undo/redo. |
| `src/keymap.rs` | Application keys; execution shortcuts are not registered here. |
| `src/model.rs` | Caller metadata and configurable resource limits. |
| `src/theme.rs` | Semantic styles; Auto preserves terminal colors without palette polling. |
| `src/transcript.rs` | Stable display entries, cached wrapping, logical-line scroll anchors. |
| `src/render.rs` | Responsive header, transcript, composer, footer and theme overlay. |
| `src/sanitize.rs` | Removes raw control characters from untrusted cell content; only renderer writes ANSI. |
| `src/approval.rs` | UI permission response values and case-preserving feedback parsing. |
| `tests/agent_interaction.rs` | Approval focus, long-operation review and completion behavior. |
| `tests/behavior.rs` | Deterministic interaction and rendered-buffer checks. |
| `tests/interactive.rs` | Ignored, explicitly labelled developer preview; no AI/tool execution. |

## Interaction

| Key | Behavior |
| --- | --- |
| Enter | Returns a Submit action for nonblank draft; release/repeat does not resubmit. |
| ? on empty input | Opens shortcut help. PgUp/PgDn scroll it; ? or Esc closes it. In a draft or permission response, ? remains text. Pasted ? never opens help. |
| Ctrl+O | Expands/collapses retained thinking; does not change model thinking settings. |
| Alt+Enter, Shift+Enter, Ctrl+J | Inserts a newline. Modifier delivery depends on the terminal. |
| F2 | Opens theme preview. Arrows preview; Enter applies for this App instance; Esc restores the original. |
| Arrow keys, Home, End | Editor navigation through the widget. |
| Ctrl+Z / Ctrl+Y | Draft undo / redo. |
| PageUp / PageDown | Scrolls the transcript; reading old output stops following appended entries. |
| Ctrl+End | Follows latest transcript output again. |
| Esc / Ctrl+C | Closes the theme overlay first, otherwise returns Cancel; the caller decides cancellation. |
| Ctrl+Q | Returns Exit. The caller closes the screen. |

Paste is text insertion, including `/` or `!` prefixes. It never dispatches a command. Pasted
control characters cannot issue screen/clipboard/title instructions. Escape-sequence payload text
may remain visible literally; this is not a terminal emulator. Draft limits reject the whole insertion
and retain the old text. Text is not trimmed when submitting. Theme/draft state is currently in RAM.

## Running the agent and validating the library

The user-facing command is `cluaiz agent`, in an existing CMD/PowerShell/shell tab. Use
`cluaiz agent --ui plain` for the previous line-oriented interface. There is no second terminal
application or separate CLI executable. See [the agent guide](../docs/cmd/agent.md).

From the repository root:

```sh
cargo test --release -p cluaiz-terminal
cargo clippy --release -p cluaiz-terminal --all-targets -- -D warnings
```

`tests/interactive.rs` is an ignored developer-only terminal restoration test with labelled
fixtures. It is not the way to start Cluaiz or test model behavior; the Rust test harness can print
its own progress message over a long-running test. Use the production command for live UI review.

The separate `test_real_terminal_cleanup_after_error_and_panic` ignored test checks caller-error,
expected-panic, exclusive-screen and reopen behavior. Its panic text is intentional; the test must
still finish with a passing result. Run ignored terminal tests individually to avoid screen contention.
The automated tool console sets `TERM=dumb`; a test-owned interactive pseudo-console must set its
own real terminal type. Do not globally override TERM or force rich UI on redirected output.

Resolved UI dependencies require a newer shared `bitflags` than the previous lockfile; Cargo.lock
records that resolution. The existing CLI's Crossterm version remains unchanged. P1's package tests
do not constitute a full Engine/CLI native build or API regression pass.

## Failure and recovery

- Terminal setup/drawing/explicit cleanup returns I/O errors. Drop and panic cleanup are best effort;
  use `close()` to observe cleanup errors. Normal process unwinding is required for Drop.
- Input and display limits return notices/errors. The caller owns durable logs and old-history loading.
- Tiny windows show a recoverable resize notice. Wrapped content is rebuilt on width/content changes;
  when reading older output the logical source line is retained, not an exact byte/pixel position.
- Clipboard image/file attachment ingress, mouse capture, selection/export UI, Markdown styling, persistent theme/history, daemon attach,
  installed MCP/plugin/skill integration and background task views remain later milestones.
