//! Virtuelle CDJs aus Audiodateien: Datei dekodieren und so analysieren, wie es
//! rekordbox für den Stick tut (Länge, Tempo, erster Beat, farbige Wellenform,
//! Tags). Die Analyse ist bewusst einfach; sie soll realistische Testdaten liefern,
//! keine Studio-Genauigkeit.

use crate::deck::SimTrack;
use decklight_library::{
    CueKind, CuePoint, GridBeat, TrackData, TrackMetadata, WaveColumn, Waveform,
};
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, MetadataRevision, StandardTagKey};
use symphonia::core::probe::Hint;

/// Spalten pro Sekunde der Detail-Wellenform (wie bei rekordbox).
pub const COLUMNS_PER_SECOND: f64 = 150.0;

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("{0}: {1}")]
    Io(String, std::io::Error),
    #[error("{0}: Format nicht lesbar ({1})")]
    Decode(String, String),
    #[error("{0}: keine Audiospur")]
    NoTrack(String),
    #[error("{0}: zu kurz für eine Analyse")]
    TooShort(String),
}

/// Dekodiertes Audio, auf Mono gemischt.
pub struct Decoded {
    pub samples: Vec<f32>,
    pub rate: u32,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
}

/// Ergebnis der Analyse.
#[derive(Debug, Clone, PartialEq)]
pub struct Analysis {
    pub length_ms: u32,
    pub bpm: f64,
    pub first_beat_ms: f64,
    pub waveform: Waveform,
}

pub fn decode(path: &Path) -> Result<Decoded, AudioError> {
    let name = path.display().to_string();
    let file = std::fs::File::open(path).map_err(|e| AudioError::Io(name.clone(), e))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let dec_err = |e: SymError| AudioError::Decode(name.clone(), e.to_string());
    let mut probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(dec_err)?;
    let mut tags = Tags::default();
    if let Some(rev) = probed.metadata.get().as_ref().and_then(|m| m.current()) {
        tags.read(rev);
    }
    let mut format = probed.format;
    if let Some(rev) = format.metadata().current() {
        tags.read(rev);
    }
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| AudioError::NoTrack(name.clone()))?;
    let track_id = track.id;
    let rate = track.codec_params.sample_rate.unwrap_or(44_100);
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(dec_err)?;
    let mut samples = Vec::new();
    let mut buf: Option<SampleBuffer<f32>> = None;
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => break,
            Err(e) => return Err(dec_err(e)),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(SymError::DecodeError(_)) => continue,
            Err(e) => return Err(dec_err(e)),
        };
        let spec = *decoded.spec();
        let channels = spec.channels.count().max(1);
        let need = decoded.capacity() as u64;
        if buf
            .as_ref()
            .is_none_or(|b| (b.capacity() as u64) < need * channels as u64)
        {
            buf = Some(SampleBuffer::new(need, spec));
        }
        let Some(b) = buf.as_mut() else { continue };
        b.copy_interleaved_ref(decoded);
        for frame in b.samples().chunks(channels) {
            samples.push(frame.iter().sum::<f32>() / channels as f32);
        }
    }
    Ok(Decoded {
        samples,
        rate,
        title: tags.title,
        artist: tags.artist,
        album: tags.album,
        genre: tags.genre,
    })
}

#[derive(Default)]
struct Tags {
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    genre: Option<String>,
}

impl Tags {
    fn read(&mut self, rev: &MetadataRevision) {
        for tag in rev.tags() {
            let slot = match tag.std_key {
                Some(StandardTagKey::TrackTitle) => &mut self.title,
                Some(StandardTagKey::Artist) => &mut self.artist,
                Some(StandardTagKey::Album) => &mut self.album,
                Some(StandardTagKey::Genre) => &mut self.genre,
                _ => continue,
            };
            let v = tag.value.to_string();
            if slot.is_none() && !v.trim().is_empty() {
                *slot = Some(v.trim().to_string());
            }
        }
    }
}

/// Einpoliger Tiefpass, Koeffizient für Grenzfrequenz `hz`.
fn lowpass_coeff(hz: f64, rate: u32) -> f32 {
    (1.0 - (-2.0 * std::f64::consts::PI * hz / f64::from(rate)).exp()) as f32
}

pub fn analyze(samples: &[f32], rate: u32) -> Option<Analysis> {
    let rate_f = f64::from(rate.max(1));
    let length_ms = (samples.len() as f64 / rate_f * 1000.0) as u32;
    if length_ms < 4_000 {
        return None;
    }
    // Drei Bänder: tief (< 200 Hz), mittel, hoch (> 2,5 kHz).
    let a_low = lowpass_coeff(200.0, rate);
    let a_mid = lowpass_coeff(2_500.0, rate);
    let (mut lp1, mut lp2) = (0.0f32, 0.0f32);
    let per_col = rate_f / COLUMNS_PER_SECOND;
    let n_cols = (samples.len() as f64 / per_col).floor() as usize;
    let hop = (rate / 1000).max(1) as usize;
    let mut env = Vec::with_capacity(samples.len() / hop + 1);
    let (mut env_acc, mut env_n) = (0.0f64, 0usize);
    let mut cols: Vec<[f64; 4]> = vec![[0.0; 4]; n_cols];
    for (i, &x) in samples.iter().enumerate() {
        lp1 += a_low * (x - lp1);
        lp2 += a_mid * (x - lp2);
        let (low, mid, high) = (lp1, lp2 - lp1, x - lp2);
        let c = (i as f64 / per_col) as usize;
        if let Some(col) = cols.get_mut(c) {
            col[0] = col[0].max(f64::from(x.abs()));
            col[1] += f64::from(low * low);
            col[2] += f64::from(mid * mid);
            col[3] += f64::from(high * high);
        }
        // Einsatz-Hüllkurve (1 ms) vor allem aus dem Bass.
        env_acc += f64::from(low * low) + 0.25 * f64::from(mid * mid);
        env_n += 1;
        if env_n == hop {
            env.push(env_acc / hop as f64);
            env_acc = 0.0;
            env_n = 0;
        }
    }
    let waveform = waveform(&cols, per_col);
    // Ein Hüllkurven-Schritt dauert hop/rate Sekunden (nicht immer genau 1 ms).
    let env_ms = hop as f64 * 1000.0 / rate_f;
    let (bpm, first_beat_ms) = tempo(&env, env_ms);
    Some(Analysis {
        length_ms,
        bpm,
        first_beat_ms,
        waveform,
    })
}

/// Wellenform im CDJ-3000-Stil: je Spalte die Pegel der drei Bänder (Bass blau,
/// Mitten orange, Höhen weiß), alle auf denselben Spitzenwert bezogen.
fn waveform(cols: &[[f64; 4]], per_col: f64) -> Waveform {
    let mut peaks: Vec<f64> = cols.iter().map(|c| c[0]).collect();
    peaks.sort_by(f64::total_cmp);
    let norm = peaks
        .get(peaks.len() * 99 / 100)
        .copied()
        .unwrap_or(1.0)
        .max(1e-6);
    // Effektivwert je Band im Verhältnis zur Spitze; Gewichte wie auf dem
    // Player: Bass füllt, Mitten mittel, Höhen als schmaler Kern.
    const GAIN: [f64; 3] = [1.6, 1.4, 1.2];
    let bands: Vec<[u8; 3]> = cols
        .iter()
        .map(|c| {
            let level = |k: usize| {
                let rms = (c[k + 1] / per_col.max(1.0)).sqrt();
                ((rms / norm * GAIN[k]).min(1.0) * 31.0).round() as u8
            };
            [level(0), level(1), level(2)]
        })
        .collect();
    Waveform {
        per_second: COLUMNS_PER_SECOND,
        columns: bands.iter().map(|b| WaveColumn::from_bands(*b)).collect(),
        bands,
    }
}

/// Tempo und Phase aus der 1-ms-Hüllkurve: Einsatzstärke, Autokorrelation für die
/// grobe Periode, dann feine Suche über gefaltete Histogramme (Tempo und Phase).
fn tempo(env: &[f64], env_ms: f64) -> (f64, f64) {
    // Einsatzstärke: positive Änderung der log-komprimierten Energie, Mittelwert ab.
    let log: Vec<f64> = env.iter().map(|e| (1.0 + 1e4 * e).ln()).collect();
    let mut onset: Vec<f64> = log.windows(2).map(|w| (w[1] - w[0]).max(0.0)).collect();
    onset.insert(0, 0.0);
    let k = 50;
    let mut out = vec![0.0; onset.len()];
    let mut sum: f64 = onset.iter().take(2 * k).sum();
    for i in 0..onset.len() {
        if i + k < onset.len() {
            sum += onset[i + k];
        }
        if i > k {
            sum -= onset[i - k - 1];
        }
        let n = (i + k).min(onset.len() - 1) - i.saturating_sub(k) + 1;
        out[i] = (onset[i] - sum / n as f64).max(0.0);
    }
    let onset = out;
    // Für die Suche reichen die ersten 4 Minuten.
    let onset = &onset[..onset.len().min(240_000)];

    // Grob: Autokorrelation über 60–190 BPM auf einer geglätteten Kurve (die
    // Einsätze sind nur 1–2 ms breit und träfen sonst ganzzahlige Verschiebungen
    // nur zufällig), mit leichter Vorliebe für ~125 BPM.
    let smooth = box_filter(&box_filter(onset, 6), 6);
    let onset_fine = onset;
    let onset = &smooth[..];
    let (min_lag, max_lag) = (316usize, 1000usize);
    let mut best = (0.0f64, 500usize);
    for lag in min_lag..=max_lag.min(onset.len() / 2) {
        let mut s = 0.0;
        let mut i = 0;
        while i + lag < onset.len() {
            s += onset[i] * onset[i + lag];
            i += 2;
        }
        let bpm = 60_000.0 / lag as f64;
        let prior = (-(bpm / 125.0).log2().powi(2) / (2.0 * 0.6f64.powi(2))).exp();
        let score = s * prior;
        if score > best.0 {
            best = (score, lag);
        }
    }
    // Fein: Periode in 0,02-ms-Schritten ±1,5 %, Phase aus dem gefalteten Histogramm.
    let coarse = best.1 as f64;
    let mut fine = (f64::MIN, coarse, 0.0);
    let steps = (coarse * 0.015 / 0.02) as i64;
    for s in -steps..=steps {
        let period = coarse + s as f64 * 0.02;
        let bins = period.floor() as usize;
        let mut hist = vec![0.0f64; bins + 1];
        for (i, &v) in onset_fine.iter().enumerate() {
            if v > 0.0 {
                let b = (i as f64 % period) as usize;
                hist[b.min(bins)] += v;
            }
        }
        // Über ±2 ms glätten, damit Rundungen der Bins nicht zählen.
        let (score, phase) = (0..hist.len())
            .map(|b| {
                let s: f64 = (0..5)
                    .map(|d| hist[(b + hist.len() + d - 2) % hist.len()])
                    .sum();
                (s, b as f64)
            })
            .fold((f64::MIN, 0.0), |a, b| if b.0 > a.0 { b } else { a });
        if score > fine.0 {
            fine = (score, period, phase);
        }
    }
    let (_, period, phase) = fine;
    let (period, phase) = (period * env_ms, phase * env_ms);
    let mut bpm = 60_000.0 / period;
    // In den Bereich bringen, den DJ-Software anzeigt.
    while bpm < 78.0 {
        bpm *= 2.0;
    }
    while bpm >= 180.0 {
        bpm /= 2.0;
    }
    let beat = 60_000.0 / bpm;
    ((bpm * 100.0).round() / 100.0, phase % beat)
}

/// Gleitender Mittelwert über ±`r` Werte.
fn box_filter(x: &[f64], r: usize) -> Vec<f64> {
    let mut out = Vec::with_capacity(x.len());
    let mut sum: f64 = x.iter().take(r).sum();
    for i in 0..x.len() {
        if i + r < x.len() {
            sum += x[i + r];
        }
        if i > r {
            sum -= x[i - r - 1];
        }
        out.push(sum / (2 * r + 1) as f64);
    }
    out
}

impl SimTrack {
    /// Track aus einer Audiodatei: Länge, BPM und erster Beat aus der Analyse.
    pub fn from_audio(id: u32, path: &Path) -> Result<(SimTrack, TrackData), AudioError> {
        let d = decode(path)?;
        let a = analyze(&d.samples, d.rate)
            .ok_or_else(|| AudioError::TooShort(path.display().to_string()))?;
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Track")
            .to_string();
        // Dateiname "Artist - Titel", falls Tags fehlen.
        let (f_artist, f_title) = match stem.split_once(" - ") {
            Some((a, t)) => (a.trim().to_string(), t.trim().to_string()),
            None => (String::new(), stem.clone()),
        };
        let track = SimTrack {
            id,
            title: d.title.clone().unwrap_or(f_title),
            artist: d.artist.clone().unwrap_or(f_artist),
            length_ms: a.length_ms,
            bpm: a.bpm,
            first_beat_ms: a.first_beat_ms,
            file: Some(path.to_path_buf()),
        };
        let mut data = track.synthetic_data();
        data.metadata.album = d.album.unwrap_or_default();
        data.metadata.genre = d.genre.unwrap_or_default();
        data.metadata.key = String::new();
        data.waveform = Some(a.waveform);
        data.cues = cues_every_32_bars(&track, &data.beat_grid);
        Ok((track, data))
    }
}

/// Ein Memory Cue auf dem ersten Beat, Hot Cues A–D alle 32 Takte.
fn cues_every_32_bars(track: &SimTrack, grid: &[GridBeat]) -> Vec<CuePoint> {
    let mut cues = vec![CuePoint {
        kind: CueKind::Memory,
        position_ms: track.first_beat_ms.round() as u32,
        loop_end_ms: None,
        color: None,
        comment: String::new(),
    }];
    for slot in 1..=4u8 {
        if let Some(b) = grid.get(usize::from(slot) * 128) {
            cues.push(CuePoint {
                kind: CueKind::Hot(slot),
                position_ms: b.time_ms,
                loop_end_ms: None,
                color: None,
                comment: String::new(),
            });
        }
    }
    cues
}

/// Kurzbeschreibung fürs Terminal.
pub fn describe(meta: &TrackMetadata, a_bpm: f64, first_beat_ms: f64) -> String {
    format!(
        "{} – {} ({}:{:02}, {:.2} BPM, erster Beat {:.0} ms)",
        meta.artist,
        meta.title,
        meta.duration_s / 60,
        meta.duration_s % 60,
        a_bpm,
        first_beat_ms
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetischer Track: Kick (60-Hz-Sinus, abklingend) auf jedem Beat, Hi-Hat
    /// (Rauschen) auf den Offbeats, leises Rauschbett.
    fn synth(bpm: f64, first_beat_ms: f64, secs: f64, rate: u32) -> Vec<f32> {
        let n = (secs * f64::from(rate)) as usize;
        let beat = 60.0 / bpm;
        let mut seed = 12345u32;
        (0..n)
            .map(|i| {
                let t = i as f64 / f64::from(rate);
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let noise = (f64::from(seed) / f64::from(u32::MAX)) * 2.0 - 1.0;
                let since = (t - first_beat_ms / 1000.0).rem_euclid(beat);
                let kick = if t * 1000.0 >= first_beat_ms {
                    (2.0 * std::f64::consts::PI * 60.0 * since).sin() * (-since * 25.0).exp()
                } else {
                    0.0
                };
                let off = (since - beat / 2.0).abs();
                let hat = if off < 0.03 {
                    noise * 0.3 * (1.0 - off / 0.03)
                } else {
                    0.0
                };
                (0.8 * kick + hat + 0.02 * noise) as f32
            })
            .collect()
    }

    #[test]
    fn finds_tempo_and_first_beat() {
        for (bpm, first) in [
            (128.0, 250.0),
            (124.0, 40.0),
            (131.5, 90.0),
            (140.0, 600.0),
            (96.5, 120.0),
            (174.0, 300.0),
        ] {
            let s = synth(bpm, first, 40.0, 22_050);
            let a = analyze(&s, 22_050).unwrap();
            assert!((a.bpm - bpm).abs() < 0.05, "{bpm}: {}", a.bpm);
            let beat = 60_000.0 / bpm;
            let d = (a.first_beat_ms - first).rem_euclid(beat);
            let err = d.min(beat - d);
            assert!(
                err < 8.0,
                "{bpm}: erster Beat {} statt {first}",
                a.first_beat_ms
            );
            assert_eq!(a.length_ms, 40_000);
            assert_eq!(a.waveform.columns.len(), 40 * 150);
        }
    }

    #[test]
    fn waveform_colors_follow_bands() {
        let s = synth(120.0, 0.0, 8.0, 22_050);
        let a = analyze(&s, 22_050).unwrap();
        // Auf dem Kick dominiert der Bass (blau), auf der Hi-Hat die Höhen.
        let w = &a.waveform;
        assert_eq!(w.bands.len(), w.columns.len());
        let (kick, hat) = (150 * 2 + 3, 150 * 2 + 38);
        let [kl, _, kh] = w.bands[kick];
        let [hl, _, hh] = w.bands[hat];
        assert!(
            kl > 2 * hl.max(1),
            "Kick {:?}, Hat {:?}",
            w.bands[kick],
            w.bands[hat]
        );
        assert!(hh > kh, "Kick {:?}, Hat {:?}", w.bands[kick], w.bands[hat]);
        assert!(w.columns[kick].height > w.columns[hat].height);
        assert!(w.columns[kick].rgb[2] > w.columns[kick].rgb[0]);
        assert!(w.columns[hat].rgb[0] > w.columns[kick].rgb[0]);
    }

    #[test]
    fn decodes_wav_with_tags_from_filename() {
        let dir = std::env::temp_dir().join(format!("decklight-audio-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Test Artist - Test Title.wav");
        let s = synth(126.0, 100.0, 20.0, 22_050);
        write_wav(&path, &s, 22_050);
        let (track, data) = SimTrack::from_audio(9, &path).unwrap();
        assert_eq!(track.title, "Test Title");
        assert_eq!(track.artist, "Test Artist");
        assert!((track.bpm - 126.0).abs() < 0.05, "{}", track.bpm);
        assert_eq!(data.metadata.duration_s, 20);
        assert!(data.beat_grid.len() > 35);
        assert!(data.waveform.unwrap().columns.len() >= 2990);
        let _ = std::fs::remove_dir_all(dir);
    }

    fn write_wav(path: &Path, s: &[f32], rate: u32) {
        let mut b = Vec::new();
        let data_len = (s.len() * 2) as u32;
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + data_len).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&rate.to_le_bytes());
        b.extend_from_slice(&(rate * 2).to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&data_len.to_le_bytes());
        for v in s {
            b.extend_from_slice(&((v.clamp(-1.0, 1.0) * 32_000.0) as i16).to_le_bytes());
        }
        std::fs::write(path, b).unwrap();
    }
}
