//! Turn raw terminal captures into readable text.
//!
//! Job logs are written by `tmux pipe-pane`, which records everything the pane
//! received: shell prompts, ANSI colour/style escapes, OSC title changes, and
//! the carriage-return repaints produced by progress bars. Printing that file
//! verbatim is technically accurate but practically unreadable, so callers
//! clean it first.
//!
//! [`TerminalCleaner`] is a small terminal state machine. It drops escape
//! sequences and replays `\r`, `\b`, and erase-line semantics, so a progress
//! bar collapses to its final frame instead of repeating hundreds of times.
//! It is incremental, which lets `gjob log --follow` clean bytes as they are
//! appended without re-reading the file.

use std::io;

/// Streaming cleaner for terminal capture text.
#[derive(Debug, Default)]
pub struct TerminalCleaner {
    state: State,
    params: String,
    line: Vec<char>,
    col: usize,
    /// Treat `\r` as a line break instead of a cursor-home move.
    cr_breaks_line: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum State {
    #[default]
    Normal,
    /// Saw `ESC`; the next character selects the sequence type.
    Escape,
    /// Inside a CSI sequence (`ESC [ ... final`).
    Csi,
    /// Inside an OSC/DCS/PM/APC string, terminated by BEL or ST.
    Osc { escaped: bool },
    /// Inside a charset designator (`ESC ( B`, ...); one payload byte follows.
    Charset,
}

impl TerminalCleaner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Like [`new`](Self::new), but treat `\r` as a line break.
    ///
    /// Progress bars repaint a line with `\r`; the default collapses each
    /// repaint into the final frame, while this mode keeps every frame on its
    /// own line (useful when the intermediate frames matter).
    pub fn with_line_breaks_on_cr() -> Self {
        Self {
            cr_breaks_line: true,
            ..Self::default()
        }
    }

    /// Feed captured text, calling `emit` once per completed line with the
    /// trailing newline removed. A line that is still in progress is kept for
    /// the next call (or for [`finish`](Self::finish)).
    pub fn feed<F>(&mut self, chunk: &str, emit: &mut F) -> io::Result<()>
    where
        F: FnMut(&str) -> io::Result<()>,
    {
        for ch in chunk.chars() {
            match self.state {
                State::Normal => match ch {
                    '\u{1b}' => self.state = State::Escape,
                    '\n' => {
                        let line = self.take_line();
                        emit(&line)?;
                    }
                    '\r' if self.cr_breaks_line => {
                        let line = self.take_line();
                        emit(&line)?;
                    }
                    '\r' => self.col = 0,
                    '\u{8}' => self.col = self.col.saturating_sub(1),
                    '\t' => self.put(ch),
                    c if c.is_control() => {}
                    c => self.put(c),
                },
                State::Escape => match ch {
                    '[' => {
                        self.params.clear();
                        self.state = State::Csi;
                    }
                    ']' | 'P' | 'X' | '^' | '_' => self.state = State::Osc { escaped: false },
                    '(' | ')' | '*' | '+' | '-' | '.' | '/' | '#' | '%' => {
                        self.state = State::Charset
                    }
                    _ => self.state = State::Normal,
                },
                State::Csi => {
                    if ('@'..='~').contains(&ch) {
                        self.apply_csi(ch);
                        self.state = State::Normal;
                    } else {
                        self.params.push(ch);
                    }
                }
                State::Osc { escaped } => match (escaped, ch) {
                    (_, '\u{7}') => self.state = State::Normal,
                    (true, '\\') => self.state = State::Normal,
                    (_, '\u{1b}') => self.state = State::Osc { escaped: true },
                    _ => self.state = State::Osc { escaped: false },
                },
                State::Charset => self.state = State::Normal,
            }
        }

        Ok(())
    }

    /// Emit the final, unterminated line, if the capture ended mid-line.
    pub fn finish<F>(&mut self, emit: &mut F) -> io::Result<()>
    where
        F: FnMut(&str) -> io::Result<()>,
    {
        if self.line.is_empty() {
            return Ok(());
        }
        let line = self.take_line();
        emit(&line)
    }

    /// Write `ch` at the current cursor column, overwriting any stale text.
    fn put(&mut self, ch: char) {
        if self.col < self.line.len() {
            self.line[self.col] = ch;
        } else {
            self.line.resize(self.col, ' ');
            self.line.push(ch);
        }
        self.col += 1;
    }

    /// Take the current line, clearing the cursor and trailing padding.
    fn take_line(&mut self) -> String {
        let line: String = self.line.drain(..).collect();
        self.col = 0;
        line.trim_end_matches([' ', '\t']).to_string()
    }

    fn apply_csi(&mut self, final_byte: char) {
        match final_byte {
            // Erase in line: 0 (default) to end, 1 to start, 2 whole line.
            'K' => match self.first_param(0) {
                0 => self.line.truncate(self.col.min(self.line.len())),
                1 => {
                    for cell in self.line.iter_mut().take(self.col) {
                        *cell = ' ';
                    }
                }
                2 => {
                    self.line.clear();
                    self.col = 0;
                }
                _ => {}
            },
            // Cursor horizontal absolute (1-based).
            'G' => self.col = self.first_param(1).saturating_sub(1),
            // Cursor forward.
            'C' => self.col = self.col.saturating_add(self.first_param(1)),
            // Cursor backward.
            'D' => self.col = self.col.saturating_sub(self.first_param(1)),
            _ => {}
        }
    }

    fn first_param(&self, default: usize) -> usize {
        let first = self.params.split(';').next().unwrap_or("");
        let digits: String = first.chars().take_while(char::is_ascii_digit).collect();
        if digits.is_empty() {
            default
        } else {
            digits.parse().unwrap_or(default)
        }
    }
}

/// Clean a complete capture. Lines are joined with `\n` and the result has no
/// escape sequences, carriage returns, or other control characters.
pub fn clean_terminal_text(text: &str) -> String {
    let mut cleaner = TerminalCleaner::new();
    let mut lines: Vec<String> = Vec::new();
    let mut push = |line: &str| {
        lines.push(line.to_string());
        Ok(())
    };

    // The closure never fails, so the io::Result is only part of the API.
    cleaner.feed(text, &mut push).expect("infallible sink");
    cleaner.finish(&mut push).expect("infallible sink");

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::{clean_terminal_text, TerminalCleaner};

    #[test]
    fn removes_csi_and_osc_sequences() {
        let input = "\u{1b}[31mred\u{1b}[0m \u{1b}]0;title\u{7}done";
        assert_eq!(clean_terminal_text(input), "red done");
    }

    #[test]
    fn collapses_carriage_return_repaints_to_the_final_frame() {
        let input = "train: 0%\rtrain: 50%\rtrain: 100%";
        assert_eq!(clean_terminal_text(input), "train: 100%");
    }

    #[test]
    fn shorter_repaint_overwrites_only_its_own_cells() {
        let input = "long line\rok";
        assert_eq!(clean_terminal_text(input), "okng line");
    }

    #[test]
    fn erase_line_discards_the_overwritten_tail() {
        // tqdm redraws use ESC[K to wipe a previously longer bar.
        let input = "100%|####|\u{1b}[K\nnext";
        assert_eq!(clean_terminal_text(input), "100%|####|\nnext");
    }

    #[test]
    fn drops_cursor_moves_and_other_control_bytes() {
        let input = "a\u{1b}[2Cb\u{7f}c";
        assert_eq!(clean_terminal_text(input), "a  bc");
    }

    #[test]
    fn streaming_matches_one_shot_for_split_input() {
        let input = "one\n\u{1b}[32mtwo\u{1b}[0m\nthr";
        let expected = clean_terminal_text(input);

        let mut cleaner = TerminalCleaner::new();
        let mut lines: Vec<String> = Vec::new();
        let mut push = |line: &str| {
            lines.push(line.to_string());
            Ok(())
        };
        for ch in input.chars() {
            cleaner
                .feed(&ch.to_string(), &mut push)
                .expect("infallible sink");
        }
        cleaner.finish(&mut push).expect("infallible sink");

        assert_eq!(lines.join("\n"), expected);
    }
}
