use crate::model::{now_ms, LogLine, Stream};
use std::collections::VecDeque;

/// Keep the tail of each server's output in memory. Bounded on both axes so a
/// runaway process can't grow the app's footprint.
const MAX_LINES: usize = 2_000;
const MAX_LINE_CHARS: usize = 4_000;

pub struct Ring {
    buf: VecDeque<LogLine>,
    next_seq: u64,
    /// Highest seq already pushed to subscribers; the flusher uses this to
    /// send only what's new.
    pub emitted_seq: u64,
}

impl Default for Ring {
    fn default() -> Self {
        Self::new()
    }
}

impl Ring {
    pub fn new() -> Self {
        Ring {
            buf: VecDeque::with_capacity(256),
            next_seq: 0,
            emitted_seq: 0,
        }
    }

    pub fn push(&mut self, stream: Stream, text: &str) {
        let mut text = text.trim_end_matches(['\r', '\n']).to_string();
        if text.chars().count() > MAX_LINE_CHARS {
            text = text.chars().take(MAX_LINE_CHARS).collect::<String>() + " …";
        }
        let line = LogLine {
            seq: self.next_seq,
            ts: now_ms(),
            stream,
            text,
        };
        self.next_seq += 1;
        if self.buf.len() == MAX_LINES {
            self.buf.pop_front();
        }
        self.buf.push_back(line);
    }

    pub fn has_pending(&self) -> bool {
        self.next_seq > self.emitted_seq
    }

    /// Drain everything newer than `emitted_seq` and mark it sent.
    pub fn take_pending(&mut self) -> Vec<LogLine> {
        let out: Vec<LogLine> = self
            .buf
            .iter()
            .filter(|l| l.seq >= self.emitted_seq)
            .cloned()
            .collect();
        self.emitted_seq = self.next_seq;
        out
    }

    /// The last `n` lines, oldest first — used when the UI opens a server.
    pub fn tail(&self, n: usize) -> Vec<LogLine> {
        let skip = self.buf.len().saturating_sub(n);
        self.buf.iter().skip(skip).cloned().collect()
    }

    pub fn clear(&mut self) {
        self.buf.clear();
        self.emitted_seq = self.next_seq;
    }
}
