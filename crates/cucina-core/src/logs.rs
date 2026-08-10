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

    pub fn next_seq(&self) -> u64 {
        self.next_seq
    }

    /// The last `n` lines numbered `seq` or higher. A task run's output shares
    /// the server's stream, so this is how a run's own slice is picked back
    /// out of it — for an agent polling a run it started.
    pub fn since(&self, seq: u64, n: usize) -> Vec<LogLine> {
        let mut out: Vec<LogLine> = self.buf.iter().filter(|l| l.seq >= seq).cloned().collect();
        if out.len() > n {
            out.drain(..out.len() - n);
        }
        out
    }

    pub fn clear(&mut self) {
        self.buf.clear();
        self.emitted_seq = self.next_seq;
    }
}

#[cfg(test)]
mod tests {
    use super::{Ring, MAX_LINES};
    use crate::model::Stream;

    #[test]
    fn numbers_lines_and_returns_them_oldest_first() {
        let mut ring = Ring::new();
        ring.push(Stream::Stdout, "one");
        ring.push(Stream::Stderr, "two");
        let all = ring.tail(10);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].seq, 0);
        assert_eq!(all[0].text, "one");
        assert_eq!(all[1].seq, 1);
        assert_eq!(all[1].stream, Stream::Stderr);
    }

    #[test]
    fn trims_the_trailing_newline_a_line_arrives_with() {
        let mut ring = Ring::new();
        ring.push(Stream::Stdout, "listening\r\n");
        assert_eq!(ring.tail(1)[0].text, "listening");
    }

    /// A runaway process must not be able to grow the app's footprint.
    #[test]
    fn drops_the_oldest_lines_past_the_cap() {
        let mut ring = Ring::new();
        for i in 0..MAX_LINES + 50 {
            ring.push(Stream::Stdout, &i.to_string());
        }
        let all = ring.tail(usize::MAX);
        assert_eq!(all.len(), MAX_LINES);
        // The first 50 fell off the front, but seq keeps counting.
        assert_eq!(all[0].text, "50");
        assert_eq!(all[0].seq, 50);
    }

    #[test]
    fn truncates_a_single_absurdly_long_line() {
        let mut ring = Ring::new();
        ring.push(Stream::Stdout, &"x".repeat(10_000));
        let text = &ring.tail(1)[0].text;
        assert!(text.ends_with(" …"));
        assert!(text.chars().count() < 5_000);
    }

    #[test]
    fn tail_returns_the_newest_n() {
        let mut ring = Ring::new();
        for i in 0..10 {
            ring.push(Stream::Stdout, &i.to_string());
        }
        let last = ring.tail(3);
        assert_eq!(last.len(), 3);
        assert_eq!(last[0].text, "7");
        assert_eq!(last[2].text, "9");
        // Asking for more than there is yields everything, not a panic.
        assert_eq!(ring.tail(500).len(), 10);
    }

    #[test]
    fn hands_each_line_to_subscribers_exactly_once() {
        let mut ring = Ring::new();
        assert!(!ring.has_pending());

        ring.push(Stream::Stdout, "a");
        assert!(ring.has_pending());
        assert_eq!(ring.take_pending().len(), 1);
        assert!(!ring.has_pending());
        assert!(ring.take_pending().is_empty());

        ring.push(Stream::Stdout, "b");
        let next = ring.take_pending();
        assert_eq!(next.len(), 1);
        assert_eq!(next[0].text, "b");
    }

    /// A run's output is interleaved with the server's, so the only thing that
    /// marks where it began is the sequence number at the moment it started.
    #[test]
    fn picks_a_runs_slice_back_out_of_the_stream() {
        let mut ring = Ring::new();
        ring.push(Stream::Stdout, "server line");
        let began = ring.next_seq();
        for i in 0..4 {
            ring.push(Stream::Stdout, &format!("run {i}"));
        }

        let slice = ring.since(began, 100);
        assert_eq!(slice.len(), 4);
        assert_eq!(slice[0].text, "run 0");

        // Capped to the newest n, like tail.
        assert_eq!(ring.since(began, 2)[0].text, "run 2");
        // A run that has printed nothing yet gets nothing, not the backlog.
        assert!(ring.since(ring.next_seq(), 100).is_empty());
    }

    /// Clearing must not replay the whole buffer to subscribers afterwards.
    #[test]
    fn clearing_leaves_nothing_pending() {
        let mut ring = Ring::new();
        ring.push(Stream::Stdout, "a");
        ring.clear();
        assert!(!ring.has_pending());
        assert!(ring.tail(10).is_empty());
    }
}
