//! The prompt: a small raw-mode line editor with a symbol palette attached.
//!
//! Typing works the way it does at any prompt. What is added is a grid of the
//! active script's symbols under the line: the arrow keys move a highlight
//! around it and Enter drops the highlighted symbol into the text. Nothing is
//! translated from Latin on the way in -- the grid shows the actual letters,
//! and picking one inserts that letter.
//!
//! The grid is hidden until an arrow key asks for it, so a line of ASCII (a
//! command, or English for a Latin-input pair) is typed without anything in the
//! way. Once it is open, Enter belongs to the palette, so Esc closes it and
//! gives Enter back to the prompt. Typing a character does the same, which
//! keeps "type a command, press Enter" working without a detour.
//!
//! Everything degrades: without a palette for the script, or without a real
//! terminal on stdin (a pipe, a redirect), this falls back to a plain read.

use std::io::{self, IsTerminal, Write};

use anyhow::Result;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    queue,
    style::{Attribute, Print, SetAttribute},
    terminal::{self, ClearType},
};

use super::{display_width, display_width_char, hangul, palette, Script};

pub enum Line {
    Text(String),
    /// End of input: Ctrl-D on an empty line, or a closed pipe.
    Eof,
    /// Ctrl-C.
    Interrupted,
}

/// Fallback terminal size, for the rare case the query fails.
const FALLBACK_SIZE: (u16, u16) = (80, 24);

pub struct Editor {
    /// Where the highlight sits, remembered across lines so a second Korean
    /// line resumes browsing where the first one left off.
    selected: usize,
}

/// Whether the arrow keys will actually offer anything for `script`, so the
/// banner and help only describe the palette when it is really there.
pub fn palette_available(script: Script) -> bool {
    palette::for_script(script).is_some() && io::stdin().is_terminal()
}

impl Editor {
    pub fn new() -> Self {
        Self { selected: 0 }
    }

    /// Read one line, offering the palette for `script`.
    pub fn read_line(&mut self, prompt: &str, script: Script) -> Result<Line> {
        let Some(palette) = palette::for_script(script) else {
            return read_plain(prompt);
        };
        if !io::stdin().is_terminal() {
            return read_plain(prompt);
        }

        // A palette that outlived a change of script would highlight a symbol
        // that is no longer there.
        if self.selected >= palette.symbols.len() {
            self.selected = 0;
        }

        let _raw = RawMode::enable()?;
        self.edit(prompt, palette, script)
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

/// State for one line being edited.
struct Buffer {
    chars: Vec<char>,
    cursor: usize,
    script: Script,
}

impl Buffer {
    /// Insert a picked symbol. At the end of the line Hangul jamo compose into
    /// syllables; anywhere else -- editing back into finished text -- they go
    /// in as they are, since there is no syllable in progress to join.
    fn insert_symbol(&mut self, symbol: char) {
        if self.script == Script::Hangul && self.cursor == self.chars.len() {
            let before = self.chars.len();
            hangul::push(&mut self.chars, symbol);
            // Composition can merge into the previous character instead of
            // adding one, or split one syllable into two.
            self.cursor = self.chars.len().max(before);
            return;
        }

        self.insert_char(symbol);
    }

    fn insert_char(&mut self, ch: char) {
        self.chars.insert(self.cursor, ch);
        self.cursor += 1;
    }

    fn backspace(&mut self) {
        // At the end of a Korean line, take back one jamo rather than a whole
        // syllable, so a wrong coda costs one keystroke to fix.
        if self.script == Script::Hangul && self.cursor == self.chars.len() {
            let mut tail = self.chars.clone();
            if hangul::backspace(&mut tail) {
                self.chars = tail;
                self.cursor = self.chars.len();
                return;
            }
        }

        if self.cursor > 0 {
            self.cursor -= 1;
            self.chars.remove(self.cursor);
        }
    }

    fn text(&self) -> String {
        self.chars.iter().collect()
    }
}

impl Editor {
    fn edit(&mut self, prompt: &str, palette: &palette::Palette, script: Script) -> Result<Line> {
        let mut buffer = Buffer {
            chars: Vec::new(),
            cursor: 0,
            script,
        };
        // Every line starts closed, so Enter always submits until it is asked
        // for. The highlight position survives; the open state does not.
        let mut open = false;

        loop {
            self.draw(prompt, &buffer, palette, open)?;

            let key = match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => key,
                // Redraw on resize; ignore everything else.
                _ => continue,
            };

            match self.apply(key, &mut buffer, palette, &mut open) {
                Action::Continue => {}
                Action::Submit => {
                    self.finish(prompt, &buffer)?;
                    return Ok(Line::Text(buffer.text()));
                }
                Action::Eof => {
                    self.finish(prompt, &buffer)?;
                    return Ok(Line::Eof);
                }
                Action::Interrupt => {
                    self.finish(prompt, &buffer)?;
                    return Ok(Line::Interrupted);
                }
            }
        }
    }

    fn apply(
        &mut self,
        key: KeyEvent,
        buffer: &mut Buffer,
        palette: &palette::Palette,
        open: &mut bool,
    ) -> Action {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        match key.code {
            KeyCode::Char('c') if ctrl => return Action::Interrupt,
            KeyCode::Char('d') if ctrl && buffer.chars.is_empty() => return Action::Eof,
            KeyCode::Char('u') if ctrl => {
                buffer.chars.clear();
                buffer.cursor = 0;
            }

            // The palette owns Enter while it is open; that is what "open"
            // means, and Esc is how it is handed back.
            KeyCode::Enter if *open => buffer.insert_symbol(palette.symbols[self.selected]),
            KeyCode::Enter => return Action::Submit,
            KeyCode::Esc => *open = false,

            // Up and Down are what open the grid in the first place.
            KeyCode::Up | KeyCode::Down if !*open => *open = true,
            KeyCode::Up => self.selected = step_row(self.selected, palette, -1),
            KeyCode::Down => self.selected = step_row(self.selected, palette, 1),

            // Left and Right move within the grid when it is open, and move
            // through the text when it is not.
            KeyCode::Left if *open => {
                self.selected = (self.selected + palette.symbols.len() - 1) % palette.symbols.len();
            }
            KeyCode::Right if *open => {
                self.selected = (self.selected + 1) % palette.symbols.len();
            }
            KeyCode::Left => buffer.cursor = buffer.cursor.saturating_sub(1),
            KeyCode::Right => buffer.cursor = (buffer.cursor + 1).min(buffer.chars.len()),

            KeyCode::Home => buffer.cursor = 0,
            KeyCode::End => buffer.cursor = buffer.chars.len(),
            KeyCode::Backspace => buffer.backspace(),
            KeyCode::Delete if buffer.cursor < buffer.chars.len() => {
                buffer.chars.remove(buffer.cursor);
            }

            KeyCode::Char(ch) if !ctrl => {
                // Typing is a statement that the next Enter is meant for the
                // prompt, not the grid.
                *open = false;
                buffer.insert_char(ch);
            }

            _ => {}
        }

        Action::Continue
    }

    /// Redraw the prompt line and everything below it.
    ///
    /// The cursor sits on the prompt line between draws, so clearing downward
    /// from it wipes the previous grid without touching earlier output.
    fn draw(
        &self,
        prompt: &str,
        buffer: &Buffer,
        palette: &palette::Palette,
        open: bool,
    ) -> Result<()> {
        let width = term_width();

        let mut out = io::stdout();
        queue!(
            out,
            cursor::MoveToColumn(0),
            terminal::Clear(ClearType::FromCursorDown)
        )?;

        let (visible, cursor_column) = visible_text(prompt, buffer, width);
        queue!(out, Print(&visible))?;

        let rows = if open {
            self.draw_palette(&mut out, palette, width)?
        } else {
            queue!(out, Print("\r\n"), Print(hint(&closed_hint(), width)))?;
            1
        };

        queue!(
            out,
            cursor::MoveUp(rows),
            cursor::MoveToColumn(cursor_column)
        )?;
        out.flush()?;
        Ok(())
    }

    /// Draw the grid and its key hint. Returns how many rows were drawn below
    /// the prompt line, so the cursor can be put back.
    fn draw_palette(
        &self,
        out: &mut io::Stdout,
        palette: &palette::Palette,
        width: usize,
    ) -> Result<u16> {
        let cell = palette.cell_width();
        let columns = grid_columns(palette, width);

        let mut rows = 0u16;
        for (index, chunk) in palette.symbols.chunks(columns).enumerate() {
            queue!(out, Print("\r\n"), Print("  "))?;
            for (offset, &symbol) in chunk.iter().enumerate() {
                let padding = cell - display_width_char(symbol) - 1;
                let selected = index * columns + offset == self.selected;

                if selected {
                    queue!(out, SetAttribute(Attribute::Reverse))?;
                }
                queue!(out, Print(format!(" {symbol}{:padding$}", "")))?;
                if selected {
                    queue!(out, SetAttribute(Attribute::Reset))?;
                }
            }
            rows += 1;
        }

        let label = format!("  {} · ↑↓←→ move · Enter insert · Esc close", palette.label);
        queue!(out, Print("\r\n"), Print(hint(&label, width)))?;
        Ok(rows + 1)
    }

    /// Leave the finished line on screen with the grid cleared away, so the
    /// scrollback reads like an ordinary prompt.
    fn finish(&self, prompt: &str, buffer: &Buffer) -> Result<()> {
        let mut out = io::stdout();
        queue!(
            out,
            cursor::MoveToColumn(0),
            terminal::Clear(ClearType::FromCursorDown),
            Print(format!("{prompt}{}\r\n", buffer.text()))
        )?;
        out.flush()?;
        Ok(())
    }
}

enum Action {
    Continue,
    Submit,
    Eof,
    Interrupt,
}

fn closed_hint() -> String {
    "  ↑/↓ to browse letters".to_string()
}

/// Truncate a hint to the terminal width so it never wraps into an extra row
/// the redraw does not know about.
fn hint(text: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0;

    for ch in text.chars() {
        let next = used + display_width_char(ch);
        if next > width.saturating_sub(1) {
            break;
        }
        out.push(ch);
        used = next;
    }

    out
}

fn grid_columns(palette: &palette::Palette, width: usize) -> usize {
    // Two columns of indent, and one spare so a full row never wraps.
    (width.saturating_sub(3).max(1) / palette.cell_width().max(1)).max(1)
}

fn term_width() -> usize {
    let (width, _) = terminal::size().unwrap_or(FALLBACK_SIZE);
    (width as usize).max(20)
}

/// Move the highlight one row up (`delta` -1) or down (`delta` 1).
///
/// The grid reflows with the terminal, so a row is however many cells fit right
/// now. Moving off the bottom wraps to the top of the same column rather than
/// stopping, which makes a long grid quick to cross from either end; the last
/// row is usually short, so cells that do not exist are stepped over.
fn step_row(selected: usize, palette: &palette::Palette, delta: isize) -> usize {
    let columns = grid_columns(palette, term_width());
    let count = palette.symbols.len();
    let rows = count.div_ceil(columns) as isize;

    let column = selected % columns;
    let mut row = (selected / columns) as isize;

    for _ in 0..rows {
        row = (row + delta).rem_euclid(rows);
        let candidate = row as usize * columns + column;
        if candidate < count {
            return candidate;
        }
    }

    selected
}

/// The slice of the line to show, plus the screen column the cursor goes in.
///
/// A line longer than the terminal scrolls horizontally rather than wrapping,
/// which keeps the prompt exactly one row tall -- the redraw counts rows to
/// find its way back, so a wrapped line would put the grid in the wrong place.
fn visible_text(prompt: &str, buffer: &Buffer, width: usize) -> (String, u16) {
    let prompt_width = display_width(prompt);
    let room = width.saturating_sub(prompt_width + 1).max(1);

    // Scroll just far enough that the cursor stays on screen.
    let mut start = 0;
    while start < buffer.cursor && text_width(&buffer.chars[start..buffer.cursor]) >= room {
        start += 1;
    }

    let mut visible = String::from(prompt);
    let mut used = 0;
    for &ch in &buffer.chars[start..] {
        let next = used + display_width_char(ch);
        if next > room {
            break;
        }
        visible.push(ch);
        used = next;
    }

    let column = prompt_width + text_width(&buffer.chars[start..buffer.cursor]);
    (visible, column as u16)
}

fn text_width(chars: &[char]) -> usize {
    chars.iter().copied().map(display_width_char).sum()
}

/// Restores the terminal even if the editor returns early.
struct RawMode;

impl RawMode {
    fn enable() -> Result<Self> {
        terminal::enable_raw_mode()?;
        Ok(RawMode)
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

/// Used for Latin pairs, which need no palette, and whenever stdin is not a
/// terminal -- raw mode has nothing to talk to there.
fn read_plain(prompt: &str) -> Result<Line> {
    print!("{prompt}");
    io::stdout().flush()?;

    let mut input = String::new();
    if io::stdin().read_line(&mut input)? == 0 {
        return Ok(Line::Eof);
    }

    Ok(Line::Text(input.trim_end_matches(['\r', '\n']).to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer(text: &str, script: Script) -> Buffer {
        let chars: Vec<char> = text.chars().collect();
        Buffer {
            cursor: chars.len(),
            chars,
            script,
        }
    }

    #[test]
    fn picked_jamo_compose_at_the_end_of_the_line() {
        let mut buf = buffer("", Script::Hangul);
        for jamo in "ㅎㅏㄴ".chars() {
            buf.insert_symbol(jamo);
        }
        assert_eq!(buf.text(), "한");
        assert_eq!(buf.cursor, buf.chars.len());
    }

    #[test]
    fn picked_jamo_are_literal_when_editing_mid_line() {
        let mut buf = buffer("한국", Script::Hangul);
        buf.cursor = 1;
        buf.insert_symbol('ㅁ');
        assert_eq!(buf.text(), "한ㅁ국");
        assert_eq!(buf.cursor, 2);
    }

    #[test]
    fn cyrillic_symbols_insert_verbatim() {
        let mut buf = buffer("", Script::Cyrillic);
        for symbol in "привет".chars() {
            buf.insert_symbol(symbol);
        }
        assert_eq!(buf.text(), "привет");
    }

    #[test]
    fn backspace_peels_jamo_then_characters() {
        let mut buf = buffer("한", Script::Hangul);
        buf.backspace();
        assert_eq!(buf.text(), "하");
        buf.backspace();
        assert_eq!(buf.text(), "ㅎ");
        buf.backspace();
        assert_eq!(buf.text(), "");

        let mut buf = buffer("да", Script::Cyrillic);
        buf.backspace();
        assert_eq!(buf.text(), "д");
    }

    #[test]
    fn wide_symbols_get_narrower_grid_rows() {
        let cyrillic = palette::for_script(Script::Cyrillic).unwrap();
        let hangul = palette::for_script(Script::Hangul).unwrap();
        assert!(grid_columns(hangul, 80) < grid_columns(cyrillic, 80));
        // Even an absurdly narrow terminal gets a usable grid.
        assert!(grid_columns(hangul, 20) >= 1);
    }

    #[test]
    fn a_grid_row_always_fits_the_terminal() {
        for width in [20usize, 40, 80, 120] {
            for palette in [
                palette::for_script(Script::Cyrillic).unwrap(),
                palette::for_script(Script::Hangul).unwrap(),
            ] {
                let row = 2 + grid_columns(palette, width) * palette.cell_width();
                assert!(row < width, "row {row} overflows width {width}");
            }
        }
    }

    #[test]
    fn the_cursor_column_tracks_double_width_text() {
        let buf = buffer("한국", Script::Hangul);
        let (visible, column) = visible_text("> ", &buf, 80);
        assert_eq!(visible, "> 한국");
        assert_eq!(column, 6);
    }

    #[test]
    fn long_lines_scroll_instead_of_wrapping() {
        let buf = buffer(&"а".repeat(60), Script::Cyrillic);
        let (visible, column) = visible_text("> ", &buf, 20);
        assert!(display_width(&visible) < 20);
        assert!(column < 20);
    }
}
