//! Erzeugt aus einer laufenden Position die Timecode-Nachrichten im richtigen Takt.
//! Rein rechnerisch: `update` wird mit der aktuellen Zeit aufgerufen (z. B. jede
//! Millisekunde) und liefert, was jetzt zu senden ist.

use crate::mtc;
use crate::timecode::{FrameRate, Timecode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopMode {
    /// Letzten Wert weiter senden (Pult hält die Position, Timecode bleibt "da").
    #[default]
    Freeze,
    /// Nach dem Stopp nichts mehr senden (Pult erkennt Timecode-Ausfall).
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GeneratorConfig {
    pub rate: FrameRate,
    pub stop_mode: StopMode,
}

impl Default for GeneratorConfig {
    fn default() -> Self {
        Self {
            rate: FrameRate::Fps25,
            stop_mode: StopMode::Freeze,
        }
    }
}

/// Was der Generator wissen muss.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TcInput {
    /// Timecode-Zeit in ms (Track-Position plus Offset). `None` = kein Master.
    pub position_ms: Option<f64>,
    pub moving: bool,
    pub reverse: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcMessage {
    ArtNet(Timecode),
    MtcQuarter([u8; 2]),
    MtcFull(Timecode),
}

#[derive(Debug, Clone)]
pub struct TimecodeGenerator {
    pub cfg: GeneratorConfig,
    last_frame: Option<i64>,
    last_qf: Option<i64>,
    was_moving: bool,
    next_freeze: f64,
}

impl TimecodeGenerator {
    pub fn new(cfg: GeneratorConfig) -> Self {
        Self {
            cfg,
            last_frame: None,
            last_qf: None,
            was_moving: false,
            next_freeze: 0.0,
        }
    }

    /// Aktueller Timecode (zuletzt gesendetes Frame).
    pub fn current(&self) -> Option<Timecode> {
        self.last_frame
            .map(|f| Timecode::from_frame_index(f, self.cfg.rate))
    }

    pub fn update(&mut self, t: f64, input: TcInput) -> Vec<TcMessage> {
        let rate = self.cfg.rate;
        let mut out = Vec::new();
        let Some(pos) = input.position_ms else {
            self.last_frame = None;
            self.last_qf = None;
            self.was_moving = false;
            return out;
        };
        let frame = Timecode::frame_index(pos, rate);
        let tc = Timecode::from_frame_index(frame, rate);
        let jumped = self.last_frame.is_some_and(|last| {
            let d = frame - last;
            if input.reverse {
                !(-2..=0).contains(&d)
            } else {
                !(0..=2).contains(&d)
            }
        });
        let started = input.moving && !self.was_moving;
        let stopped = !input.moving && self.was_moving;

        if jumped || started || stopped || self.last_frame.is_none() {
            out.push(TcMessage::MtcFull(tc));
            self.last_qf = None;
        }
        if self.last_frame != Some(frame) || stopped {
            out.push(TcMessage::ArtNet(tc));
        }

        if input.moving && !input.reverse {
            let qf_now = (pos * rate.fps() / 1000.0 * 4.0).floor() as i64;
            let from = match self.last_qf {
                Some(last) if qf_now - last <= 8 => last + 1,
                // Neu einsetzen: Durchlauf beginnt am nächsten geraden Frame mit Stück 0.
                _ => {
                    let next_start = (qf_now + 7).div_euclid(8) * 8;
                    self.last_qf = Some(next_start - 1);
                    next_start
                }
            };
            for k in from..=qf_now {
                let piece = k.rem_euclid(8) as u8;
                let seq_frame = k.div_euclid(8) * 2;
                out.push(TcMessage::MtcQuarter(mtc::quarter_frame(
                    &Timecode::from_frame_index(seq_frame, rate),
                    piece,
                )));
                self.last_qf = Some(k);
            }
        } else if input.moving && self.last_frame != Some(frame) {
            // Rückwärts: Quarter-Frames versteht kaum ein Empfänger, daher Full-Frames.
            out.push(TcMessage::MtcFull(tc));
        }

        let freeze = !input.moving && self.cfg.stop_mode == StopMode::Freeze;
        if freeze && (stopped || t >= self.next_freeze) {
            if !stopped {
                out.push(TcMessage::ArtNet(tc));
            }
            self.next_freeze = t + rate.frame_ms() / 1000.0;
        }

        self.last_frame = Some(frame);
        self.was_moving = input.moving;
        out
    }
}
