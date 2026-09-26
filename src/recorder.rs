// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Recording of an emulated screen to a cast file: a periodic keyframe, row
//! diffs in between, monotonic timestamps and optional idle compression.

use crate::emulator::{Cell, Screen, TermColor};
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::time::Instant;

/// Writes screen frames to a cast file as keyframes and row diffs.
pub struct Recorder {
    writer: BufWriter<std::fs::File>,
    start: Instant,
    base_ts: f64,
    last_ts: f64,
    prev_cells: Vec<Vec<Cell>>,
    rows: usize,
    frame_count: u64,
    last_keyframe: f64,
    idle_max: Option<f64>,
}

const KEYFRAME_INTERVAL: f64 = 10.0;

impl Recorder {
    /// Create a cast file at the path and write its header line.
    pub fn create(path: &Path, cols: usize, rows: usize) -> io::Result<Self> {
        let file = std::fs::File::create(path)?;
        let mut writer = BufWriter::new(file);

        let header = serde_json::json!({
            "version": 2,
            "width": cols,
            "height": rows,
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        });
        writeln!(writer, "{}", header)?;
        writer.flush()?;

        Ok(Self {
            writer,
            start: Instant::now(),
            base_ts: 0.0,
            last_ts: 0.0,
            prev_cells: vec![vec![Cell::default(); cols]; rows],
            rows,
            frame_count: 0,
            last_keyframe: 0.0,
            idle_max: None,
        })
    }

    /// Reopen an existing cast file and continue from its last timestamp.
    pub fn append(path: &Path, cols: usize, rows: usize) -> io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let last_ts = content
            .lines()
            .last()
            .and_then(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .and_then(|v| v.as_array().and_then(|a| a[0].as_f64()))
            .unwrap_or(0.0);

        let file = std::fs::OpenOptions::new().append(true).open(path)?;
        let writer = BufWriter::new(file);

        Ok(Self {
            writer,
            start: Instant::now(),
            base_ts: last_ts,
            last_ts,
            prev_cells: vec![vec![Cell::default(); cols]; rows],
            rows,
            frame_count: 0,
            last_keyframe: 0.0,
            idle_max: None,
        })
    }

    /// Cap the recorded gap between two frames at this many seconds.
    pub fn set_idle_max(&mut self, max: f64) {
        self.idle_max = Some(max);
    }

    /// Write a labelled comment line at the current timestamp.
    pub fn write_marker(&mut self, label: &str) -> io::Result<()> {
        let ts = self.base_ts + self.start.elapsed().as_secs_f64();
        let ts = f64::max(ts, self.last_ts);
        writeln!(self.writer, "# marker: {} t={:.3}", label, ts)?;
        Ok(())
    }

    /// Record one frame, as a keyframe when due and otherwise as a diff.
    pub fn record_frame(&mut self, screen: &Screen) -> io::Result<()> {
        let mut ts = self.base_ts + self.start.elapsed().as_secs_f64();
        ts = f64::max(ts, self.last_ts);
        if let Some(max) = self.idle_max {
            let gap = ts - self.last_ts;
            if gap > max && self.frame_count > 0 {
                ts = self.last_ts + max;
            }
        }

        let needs_keyframe = (ts - self.last_keyframe) >= KEYFRAME_INTERVAL;

        if needs_keyframe || self.frame_count == 0 {
            self.write_keyframe(screen, ts)?;
        } else {
            self.write_diff(screen, ts)?;
        }

        self.prev_cells = screen.cells.clone();
        self.rows = screen.rows;
        self.frame_count += 1;
        Ok(())
    }

    fn write_keyframe(&mut self, screen: &Screen, ts: f64) -> io::Result<()> {
        let mut output = String::new();
        output.push_str("\x1b[H\x1b[2J");

        for row in 0..screen.rows.min(self.rows) {
            output.push_str(&format!("\x1b[{};1H", row + 1));
            render_row_to_string(&mut output, &screen.cells[row], screen.cols);
        }
        output.push_str(&format!(
            "\x1b[{};{}H",
            screen.cursor_row + 1,
            screen.cursor_col + 1
        ));

        let event = serde_json::json!([ts, "o", output]);
        writeln!(self.writer, "{}", event)?;
        self.writer.flush()?;
        self.last_ts = ts;
        self.last_keyframe = ts;
        Ok(())
    }

    fn write_diff(&mut self, screen: &Screen, ts: f64) -> io::Result<()> {
        let mut output = String::new();
        let mut has_changes = false;

        for row in 0..screen.rows.min(self.rows) {
            if row < self.prev_cells.len() && self.prev_cells[row] == screen.cells[row] {
                continue;
            }
            output.push_str(&format!("\x1b[{};1H\x1b[2K", row + 1));
            render_row_to_string(&mut output, &screen.cells[row], screen.cols);
            has_changes = true;
        }

        if !has_changes {
            return Ok(());
        }

        output.push_str(&format!(
            "\x1b[{};{}H",
            screen.cursor_row + 1,
            screen.cursor_col + 1
        ));

        let event = serde_json::json!([ts, "o", output]);
        writeln!(self.writer, "{}", event)?;
        self.writer.flush()?;
        self.last_ts = ts;
        Ok(())
    }

    /// Record a resize event and reset the diff baseline to the new size.
    pub fn record_resize(&mut self, cols: usize, rows: usize) -> io::Result<()> {
        let ts = self.base_ts + self.start.elapsed().as_secs_f64();
        let ts = f64::max(ts, self.last_ts);
        let event = serde_json::json!([ts, "r", format!("{}x{}", cols, rows)]);
        writeln!(self.writer, "{}", event)?;
        self.writer.flush()?;
        self.last_ts = ts;
        self.rows = rows;
        self.prev_cells = vec![vec![Cell::default(); cols]; rows];
        Ok(())
    }

    /// Number of frames recorded so far.
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }
}

fn render_row_to_string(out: &mut String, cells: &[Cell], cols: usize) {
    let mut last_style = String::new();

    for cell in cells.iter().take(cols) {
        if cell.width == 0 {
            continue;
        }

        let style = cell_style_escape(&cell.style);
        if style != last_style {
            if !last_style.is_empty() {
                out.push_str("\x1b[0m");
            }
            if !style.is_empty() {
                out.push_str(&style);
            }
            last_style = style;
        }
        out.push(cell.ch);
    }
    if !last_style.is_empty() {
        out.push_str("\x1b[0m");
    }
}

fn cell_style_escape(style: &crate::emulator::CellStyle) -> String {
    let mut codes = Vec::new();
    if style.bold {
        codes.push("1".to_string());
    }
    if style.dim {
        codes.push("2".to_string());
    }
    if style.italic {
        codes.push("3".to_string());
    }
    if style.underline {
        codes.push("4".to_string());
    }
    if style.reverse {
        codes.push("7".to_string());
    }
    if style.hidden {
        codes.push("8".to_string());
    }
    match style.fg {
        TermColor::Indexed(i) if i < 8 => codes.push(format!("{}", 30 + i)),
        TermColor::Indexed(i) if i < 16 => codes.push(format!("{}", 90 + i - 8)),
        TermColor::Indexed(i) => codes.push(format!("38;5;{i}")),
        TermColor::Rgb(r, g, b) => codes.push(format!("38;2;{r};{g};{b}")),
        TermColor::Default => {}
    }
    match style.bg {
        TermColor::Indexed(i) if i < 8 => codes.push(format!("{}", 40 + i)),
        TermColor::Indexed(i) if i < 16 => codes.push(format!("{}", 100 + i - 8)),
        TermColor::Indexed(i) => codes.push(format!("48;5;{i}")),
        TermColor::Rgb(r, g, b) => codes.push(format!("48;2;{r};{g};{b}")),
        TermColor::Default => {}
    }
    if codes.is_empty() {
        String::new()
    } else {
        format!("\x1b[{}m", codes.join(";"))
    }
}

#[cfg(test)]
#[path = "tests/recorder.rs"]
mod tests;
