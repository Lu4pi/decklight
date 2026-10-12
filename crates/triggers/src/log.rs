//! Aktions-Log: was wann ausgelöst wurde und ob es rausging.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    /// Sekunden auf der Engine-Uhr.
    pub t: f64,
    /// Wanduhr in ms seit 1970 (für Anzeige und Export).
    pub unix_ms: u64,
    pub deck: Option<u8>,
    /// Cue- oder Triggername.
    pub source: String,
    pub actions: Vec<String>,
    /// So viel später als die exakte Cue-Position wurde ausgelöst.
    pub late_ms: f64,
    pub error: Option<String>,
}

/// Ringpuffer mit den letzten Einträgen.
#[derive(Debug, Clone)]
pub struct ActionLog {
    entries: VecDeque<LogEntry>,
    capacity: usize,
}

impl Default for ActionLog {
    fn default() -> Self {
        Self::new(2000)
    }
}

impl ActionLog {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            capacity: capacity.max(1),
        }
    }

    pub fn push(&mut self, e: LogEntry) {
        if self.entries.len() == self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(e);
    }

    pub fn entries(&self) -> impl DoubleEndedIterator<Item = &LogEntry> {
        self.entries.iter()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// CSV mit Semikolon: Zeit, Deck, Quelle, Aktionen, Verspätung, Fehler.
    pub fn to_csv(&self) -> String {
        let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
        let mut out = String::from("unix_ms;deck;source;actions;late_ms;error\n");
        for e in &self.entries {
            out.push_str(&format!(
                "{};{};{};{};{:.1};{}\n",
                e.unix_ms,
                e.deck.map_or(String::new(), |d| d.to_string()),
                q(&e.source),
                q(&e.actions.join(" | ")),
                e.late_ms,
                q(e.error.as_deref().unwrap_or("")),
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_and_csv() {
        let mut log = ActionLog::new(2);
        for i in 0u8..3 {
            log.push(LogEntry {
                t: f64::from(i),
                unix_ms: 1000 + u64::from(i),
                deck: Some(1),
                source: format!("Cue \"{i}\""),
                actions: vec!["OSC /cmd \"Go+\"".into()],
                late_ms: 1.5,
                error: None,
            });
        }
        assert_eq!(log.len(), 2);
        assert_eq!(log.entries().next().unwrap().t, 1.0);
        let csv = log.to_csv();
        assert!(csv.contains("1001;1;\"Cue \"\"1\"\"\";\"OSC /cmd \"\"Go+\"\"\";1.5;\"\""));
    }
}
