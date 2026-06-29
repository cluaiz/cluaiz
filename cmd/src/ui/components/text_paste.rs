use color_eyre::Result;
use std::io::{stdout, Write};
use colored::Colorize;
use crossterm::event::{self, Event, KeyCode, EnableBracketedPaste, DisableBracketedPaste};
use crossterm::terminal::{enable_raw_mode, disable_raw_mode};
use crossterm::execute;

/// 🧠 cluaiz Gemini-Style Paste Input
/// Handles the 'Visual vs Logical' split: 
/// - Visual: Typed chars + [Pasted Text: X lines, Y chars] slugs.
/// - Logical: The actual full string to be submitted.
pub fn read_line_cluaiz(prompt_text: &str, placeholder: &str) -> Result<Option<String>> {
    let mut logical_buffer = String::new();
    let mut has_input = false;

    print!("{} ", prompt_text.cyan().bold());
    // Initial Reactive Placeholder
    print!("{}", placeholder.bright_black());
    stdout().flush()?;

    // 🛡️ BARE-METAL LOCKDOWN: Enable raw mode and bracketed paste
    execute!(stdout(), EnableBracketedPaste)?;
    enable_raw_mode()?;

    let (_cursor_x, _cursor_y) = crossterm::cursor::position().unwrap_or((0, 0));
    let _prompt_offset = prompt_text.chars().count() + 1;

    let result = loop {
        if let Ok(event) = event::read() {
            match event {
                Event::Key(key) => match key.code {
                    KeyCode::Char(c) => {
                        if !has_input {
                            // Clear placeholder on first interaction
                            let ph_len = placeholder.chars().count();
                            print!("{}", "\x08".repeat(ph_len));
                            print!("{}", " ".repeat(ph_len));
                            print!("{}", "\x08".repeat(ph_len));
                            has_input = true;
                        }
                        logical_buffer.push(c);
                        print!("{}", c);
                        stdout().flush()?;
                    }
                    KeyCode::Backspace => {
                        if !logical_buffer.is_empty() {
                            logical_buffer.pop();
                            print!("\x08 \x08"); // standard backspace
                            
                            if logical_buffer.is_empty() {
                                has_input = false;
                                print!("{}", placeholder.bright_black());
                                stdout().flush()?;
                            } else {
                                stdout().flush()?;
                            }
                        }
                    }
                    KeyCode::Enter => {
                        print!("\r\n");
                        stdout().flush()?;
                        break Ok(Some(logical_buffer));
                    }
                    KeyCode::Esc => {
                        print!("\r\n");
                        stdout().flush()?;
                        break Ok(None);
                    }
                    _ => {}
                },
                Event::Paste(content) => {
                    if !has_input {
                        let ph_len = placeholder.chars().count();
                        print!("{}", "\x08".repeat(ph_len));
                        print!("{}", " ".repeat(ph_len));
                        print!("{}", "\x08".repeat(ph_len));
                        has_input = true;
                    }
                    
                    let char_count = content.len();
                    let line_count = content.lines().count();
                    
                    // Add full content to logical buffer (what AI sees)
                    logical_buffer.push_str(&content);
                    
                    // Only print the visual summary slug (Gemini style)
                    let slug = format!("[Pasted Text: {} lines, {} chars]", line_count, char_count);
                    print!("{}", slug.bright_black().bold()); 
                    stdout().flush()?;
                }
                _ => {}
            }
        }
    };

    // 🧹 ATOMIC CLEANUP
    let _ = disable_raw_mode();
    let _ = execute!(stdout(), DisableBracketedPaste);
    result
}
