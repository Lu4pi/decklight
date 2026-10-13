// Anzeige-Helfer für Decks: Timecode, Farbe nach Zustand, Symbol, nächster Cue.

import { conn } from "./conn.svelte";
import { toTimecode } from "./format";
import { barToMs, type LoadedTrack } from "./tracks";
import type { Cue, DeckView, FrameRate } from "./types";

export type Tone = "run" | "loop" | "stop" | "off";

/** Grün läuft, Gelb Loop, Weiß steht, Grau leer. */
export function tone(d: DeckView | undefined | null): Tone {
  if (!d || d.play_state === "no_track" || d.position_ms == null) return "off";
  if (d.play_state === "looping") return "loop";
  return d.moving ? "run" : "stop";
}

export function stateIcon(d: DeckView | undefined | null): string {
  switch (d?.play_state) {
    case "playing":
    case "cue_playing":
      return "▶";
    case "looping":
      return "⟲";
    case "paused":
      return "❚❚";
    case "searching":
    case "cue_scratch":
      return "⇆";
    case "loading":
      return "…";
    case "no_track":
    case undefined:
      return "";
    default:
      return "■";
  }
}

export function rate(): FrameRate {
  return conn.state?.timecode.rate ?? conn.settings?.timecode.generator.rate ?? "25";
}

/** Track-Timecode des Decks (mit Offset), sonst die Position als Timecode. */
export function deckTimecode(d: DeckView | undefined | null): string | null {
  if (!d || d.position_ms == null) return null;
  return d.timecode ?? toTimecode(Math.max(0, d.position_ms), rate());
}

/** Spielzeit mit Millisekunden: 3:12.564 → "03:12:564". */
export function elapsed(ms: number | null | undefined): string {
  if (ms == null || !isFinite(ms)) return "--:--:---";
  const t = Math.max(0, ms);
  const p = (n: number, w = 2) => String(Math.floor(n)).padStart(w, "0");
  return `${p(t / 60000)}:${p((t / 1000) % 60)}:${p(t % 1000, 3)}`;
}

export function pitch(d: DeckView): string {
  return `${d.pitch_percent >= 0 ? "+" : ""}${d.pitch_percent.toFixed(2)}%`;
}

export function trackLength(d: DeckView): number | null {
  return d.track_length_ms ?? (d.info ? d.info.duration_s * 1000 : null);
}

export function titleLine(d: DeckView): string {
  if (!d.info) return d.play_state === "no_track" ? "Kein Track" : "—";
  return d.info.artist ? `${d.info.artist} - ${d.info.title}` : d.info.title;
}

export interface CueState {
  current: Cue | null;
  next: Cue | null;
  /** Zeit bis zum nächsten Cue in ms (Trackzeit, ohne Pitch). */
  untilNext: number | null;
}

/** Aktueller und nächster Cue eines Tracks an einer Position. */
export function cueState(track: LoadedTrack | null, cues: Cue[], pos: number | null): CueState {
  if (pos == null) return { current: null, next: null, untilNext: null };
  const timed = cues
    .map((c) => ({ c, ms: c.position.type === "ms" ? c.position.ms : track ? barToMs(track, c.position.bar, c.position.beat) : null }))
    .filter((x): x is { c: Cue; ms: number } => x.ms != null)
    .sort((a, b) => a.ms - b.ms);
  let current: Cue | null = null;
  let next: { c: Cue; ms: number } | null = null;
  for (const x of timed) {
    if (x.ms - x.c.lead_ms <= pos) current = x.c;
    else {
      next = x;
      break;
    }
  }
  return { current, next: next?.c ?? null, untilNext: next ? next.ms - next.c.lead_ms - pos : null };
}
