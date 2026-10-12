// Track-Details (Wellenform, Beats, Cues) mit Zwischenspeicher.

import { url } from "./conn.svelte";
import type { TrackDetail } from "./types";

export interface Wave {
  perSecond: number;
  columns: number;
  /** Je Spalte: Höhe (0–31), R, G, B. */
  data: Uint8Array;
}

export interface LoadedTrack {
  detail: TrackDetail;
  wave: Wave | null;
  /** Beat-Zeiten in ms und Schlag im Takt, für das Raster. */
  beatTimes: Float64Array;
  beatInBar: Uint8Array;
}

const cache = new Map<string, Promise<LoadedTrack | null>>();

function decode(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

export function loadTrack(key: string, refresh = false): Promise<LoadedTrack | null> {
  if (!refresh && cache.has(key)) return cache.get(key)!;
  const p = fetch(url(`/api/track/${encodeURIComponent(key)}`))
    .then(async (r) => {
      if (!r.ok) {
        cache.delete(key);
        return null;
      }
      const detail = (await r.json()) as TrackDetail;
      const wave = detail.waveform
        ? { perSecond: detail.waveform.per_second, columns: detail.waveform.columns, data: decode(detail.waveform.data) }
        : null;
      return {
        detail,
        wave,
        beatTimes: Float64Array.from(detail.beats.map((b) => b[0])),
        beatInBar: Uint8Array.from(detail.beats.map((b) => b[1])),
      };
    })
    .catch(() => {
      cache.delete(key);
      return null;
    });
  cache.set(key, p);
  return p;
}

export function artworkUrl(key: string): string {
  return url(`/api/artwork/${encodeURIComponent(key)}`);
}

/** Takt/Schlag ab dem ersten Taktanfang → Millisekunden (wie im Kern). */
export function barToMs(t: LoadedTrack, bar: number, beat: number): number | null {
  let first = -1;
  for (let i = 0; i < t.beatInBar.length; i++) {
    if (t.beatInBar[i] === 1) {
      first = i;
      break;
    }
  }
  if (first < 0 || bar < 1 || beat < 1) return null;
  const idx = first + (bar - 1) * 4 + (beat - 1);
  return idx < t.beatTimes.length ? t.beatTimes[idx] : null;
}

/** Millisekunden → nächster Beat als Takt/Schlag. */
export function msToBar(t: LoadedTrack, ms: number): { bar: number; beat: number } | null {
  let first = -1;
  for (let i = 0; i < t.beatInBar.length; i++) {
    if (t.beatInBar[i] === 1) {
      first = i;
      break;
    }
  }
  if (first < 0) return null;
  let best = first;
  let bestD = Infinity;
  for (let i = first; i < t.beatTimes.length; i++) {
    const d = Math.abs(t.beatTimes[i] - ms);
    if (d < bestD) {
      bestD = d;
      best = i;
    } else if (t.beatTimes[i] > ms) break;
  }
  const n = best - first;
  return { bar: Math.floor(n / 4) + 1, beat: (n % 4) + 1 };
}
