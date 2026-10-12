// Zeit- und Timecode-Formatierung.

import type { FrameRate, PlayState } from "./types";

export function clock(ms: number | null | undefined, withTenths = true): string {
  if (ms == null || !isFinite(ms)) return "--:--";
  const neg = ms < 0;
  const t = Math.abs(ms);
  const m = Math.floor(t / 60000);
  const s = Math.floor(t / 1000) % 60;
  const d = Math.floor(t / 100) % 10;
  return `${neg ? "-" : ""}${m}:${String(s).padStart(2, "0")}${withTenths ? "." + d : ""}`;
}

export function fps(rate: FrameRate): number {
  return rate === "24" ? 24 : rate === "25" ? 25 : rate === "30" ? 30 : 30000 / 1001;
}

/** Millisekunden → HH:MM:SS:FF (ohne Drop-Frame-Korrektur, für Anzeige und Eingabe). */
export function toTimecode(ms: number, rate: FrameRate): string {
  const nominal = rate === "24" ? 24 : rate === "25" ? 25 : 30;
  const totalFrames = Math.floor((ms / 1000) * nominal + 1e-6);
  const f = totalFrames % nominal;
  const s = Math.floor(totalFrames / nominal) % 60;
  const m = Math.floor(totalFrames / nominal / 60) % 60;
  const h = Math.floor(totalFrames / nominal / 3600);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(h)}:${p(m)}:${p(s)}:${p(f)}`;
}

/** HH:MM:SS:FF → Millisekunden; `null` bei ungültiger Eingabe. */
export function parseTimecode(text: string, rate: FrameRate): number | null {
  const m = text.trim().match(/^(\d{1,2}):(\d{2}):(\d{2})[:;.](\d{2})$/);
  if (!m) return null;
  const nominal = rate === "24" ? 24 : rate === "25" ? 25 : 30;
  const [h, mi, s, f] = m.slice(1).map(Number);
  if (mi > 59 || s > 59 || f >= nominal) return null;
  return ((h * 3600 + mi * 60 + s) * nominal + f) * (1000 / nominal);
}

/** "1:23.4" oder Sekunden → Millisekunden. */
export function parseClock(text: string): number | null {
  const t = text.trim().replace(",", ".");
  const m = t.match(/^(?:(\d+):)?(\d+(?:\.\d+)?)$/);
  if (!m) return null;
  return ((m[1] ? Number(m[1]) * 60 : 0) + Number(m[2])) * 1000;
}

export function stateLabel(s: PlayState): string {
  if (typeof s === "object") return `?${s.unknown}`;
  return (
    {
      no_track: "Leer",
      loading: "Lädt",
      playing: "Play",
      looping: "Loop",
      paused: "Pause",
      cued: "Cue",
      cue_playing: "Cue-Play",
      cue_scratch: "Scratch",
      searching: "Suche",
      spun_down: "Gestoppt",
      ended: "Ende",
    } as Record<string, string>
  )[s];
}

export function time(unixMs: number): string {
  return new Date(unixMs).toLocaleTimeString("de-DE");
}

export function rgb(c: [number, number, number] | null | undefined, fallback = "#ffb020"): string {
  return c ? `rgb(${c[0]},${c[1]},${c[2]})` : fallback;
}
