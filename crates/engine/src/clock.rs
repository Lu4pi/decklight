//! Zeitbasis je Deck.
//!
//! CDJs melden die Position nur alle 30–200 ms, und die Pakete kommen mit
//! Netzwerk-Jitter an. Die Uhr rechnet zwischen den Meldungen mit dem aktuellen
//! Tempo weiter und gleicht kleine Abweichungen weich über die Laufgeschwindigkeit
//! aus, damit der Timecode nie rückwärts zuckt. Nur echte Sprünge (Cue, Hot Cue,
//! Loop-Ende, Suchen) setzen die Position hart.
//!
//! Zeiten `t` sind Sekunden auf einer beliebigen monotonen Uhr, Positionen
//! Millisekunden im Track.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ClockConfig {
    /// Abweichung, ab der eine Meldung als echter Sprung gilt.
    pub jump_threshold_ms: f64,
    /// Zeitraum, über den eine kleine Abweichung ausgeglichen wird.
    pub correction_horizon_ms: f64,
    /// Feste Verzögerung zwischen Abtastung im CDJ und Empfang (Kalibrierung).
    pub latency_ms: f64,
}

impl Default for ClockConfig {
    fn default() -> Self {
        Self {
            jump_threshold_ms: 120.0,
            correction_horizon_ms: 400.0,
            latency_ms: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Anchor {
    t: f64,
    pos_ms: f64,
    /// Nennrate (Pitch, Richtung): Track-ms pro Echtzeit-ms.
    rate: f64,
    /// Rate inklusive Korrektur, mit der zwischen den Meldungen gerechnet wird.
    run_rate: f64,
}

/// Ein harter Positionswechsel.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Jump {
    pub from_ms: f64,
    pub to_ms: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimecodeClock {
    cfg: ClockConfig,
    anchor: Option<Anchor>,
    moving: bool,
}

impl Default for TimecodeClock {
    fn default() -> Self {
        Self::new(ClockConfig::default())
    }
}

impl TimecodeClock {
    pub fn new(cfg: ClockConfig) -> Self {
        Self {
            cfg,
            anchor: None,
            moving: false,
        }
    }

    pub fn config(&self) -> ClockConfig {
        self.cfg
    }

    pub fn is_moving(&self) -> bool {
        self.moving
    }

    pub fn reset(&mut self) {
        self.anchor = None;
        self.moving = false;
    }

    /// Neue Positionsmeldung. `rate` ist die Abspielrate (1,0 = Originaltempo,
    /// negativ = rückwärts). Liefert einen Sprung, wenn die Meldung nicht zur
    /// bisherigen Bewegung passt.
    pub fn observe(&mut self, t: f64, pos_ms: f64, rate: f64, moving: bool) -> Option<Jump> {
        let pos_ms = if moving {
            pos_ms + self.cfg.latency_ms * rate
        } else {
            pos_ms
        };
        if self.anchor.is_none() {
            self.set(t, pos_ms, rate, moving);
            return None;
        }
        let predicted = self.position_at(t).unwrap_or(pos_ms);
        let err = pos_ms - predicted;

        if err.abs() > self.cfg.jump_threshold_ms {
            self.set(t, pos_ms, rate, moving);
            return Some(Jump {
                from_ms: predicted,
                to_ms: pos_ms,
            });
        }
        if moving != self.moving || !moving {
            // Start oder Stopp: Startpunkt übernehmen, kein Sprung.
            // Im Stillstand gilt die gemeldete Position unverändert.
            self.set(t, pos_ms, rate, moving);
            return None;
        }
        // Weiche Korrektur: Fehler über den Horizont abbauen, Richtung beibehalten.
        let mut run_rate = rate + err / self.cfg.correction_horizon_ms;
        if rate > 0.0 {
            run_rate = run_rate.max(rate * 0.5);
        } else if rate < 0.0 {
            run_rate = run_rate.min(rate * 0.5);
        }
        self.anchor = Some(Anchor {
            t,
            pos_ms: predicted,
            rate,
            run_rate,
        });
        None
    }

    fn set(&mut self, t: f64, pos_ms: f64, rate: f64, moving: bool) {
        self.anchor = Some(Anchor {
            t,
            pos_ms,
            rate,
            run_rate: rate,
        });
        self.moving = moving;
    }

    /// Geschätzte Track-Position zum Zeitpunkt `t`.
    pub fn position_at(&self, t: f64) -> Option<f64> {
        let a = self.anchor?;
        if !self.moving {
            return Some(a.pos_ms);
        }
        let dt_ms = (t - a.t) * 1000.0;
        // Die Korrektur gilt nur für den Horizont, danach wieder Nennrate.
        let corrected = dt_ms.min(self.cfg.correction_horizon_ms).max(0.0);
        let plain = (dt_ms - corrected).max(0.0);
        let pos = a.pos_ms + corrected * a.run_rate + plain * a.rate + dt_ms.min(0.0) * a.rate;
        Some(pos.max(0.0))
    }

    /// Aktuelle Nennrate (Pitch inklusive Richtung).
    pub fn rate(&self) -> f64 {
        self.anchor.map_or(0.0, |a| a.rate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extrapolates_between_observations() {
        let mut c = TimecodeClock::default();
        c.observe(10.0, 5000.0, 1.0, true);
        assert_eq!(c.position_at(10.5), Some(5500.0));
        c.observe(11.0, 6000.0, 1.08, true);
        assert!((c.position_at(12.0).unwrap() - 7080.0).abs() < 1e-6);
    }

    #[test]
    fn small_error_is_slewed_not_jumped() {
        let mut c = TimecodeClock::default();
        c.observe(0.0, 0.0, 1.0, true);
        // 20 ms Rückstand gemeldet: kein Sprung, Ausgabe bleibt monoton.
        assert_eq!(c.observe(1.0, 980.0, 1.0, true), None);
        let mut last = c.position_at(1.0).unwrap();
        for i in 1..=100 {
            let p = c.position_at(1.0 + f64::from(i) * 0.01).unwrap();
            assert!(p >= last, "rückwärts bei {i}: {p} < {last}");
            last = p;
        }
        // Nach dem Horizont ist der Fehler abgebaut.
        assert!((c.position_at(2.0).unwrap() - 1980.0).abs() < 0.5);
    }

    #[test]
    fn big_error_is_a_jump() {
        let mut c = TimecodeClock::default();
        c.observe(0.0, 60_000.0, 1.0, true);
        let j = c
            .observe(0.5, 30_000.0, 1.0, true)
            .expect("Sprung erwartet");
        assert!((j.from_ms - 60_500.0).abs() < 1e-6);
        assert_eq!(j.to_ms, 30_000.0);
        assert_eq!(c.position_at(1.0), Some(30_500.0));
    }

    #[test]
    fn pause_freezes_and_reverse_runs_backwards() {
        let mut c = TimecodeClock::default();
        c.observe(0.0, 1000.0, 1.0, true);
        c.observe(1.0, 2000.0, 1.0, false);
        assert_eq!(c.position_at(5.0), Some(2000.0));
        c.observe(5.0, 2000.0, -1.0, true);
        assert_eq!(c.position_at(6.0), Some(1000.0));
        assert_eq!(c.position_at(9.0), Some(0.0));
    }
}
