<script lang="ts">
  // Farbige Wellenform im CDJ-3000-Stil: Übersicht (ganzer Track) oder scrollende
  // Detailansicht um den Playhead, mit Beatgrid und Cue-Markern. Zeichnet per
  // requestAnimationFrame, damit der Playhead mit 60 fps läuft.
  import type { LoadedTrack } from "../lib/tracks";
  import type { Cue, RekordboxCue } from "../lib/types";
  import { barToMs } from "../lib/tracks";
  import { SECTION_COLOR, SECTION_LABEL } from "../lib/sections";

  interface Props {
    track: LoadedTrack | null;
    /** Aktuelle Position in ms (wird jedes Bild abgefragt). */
    position: () => number | null;
    mode?: "detail" | "overview";
    /** Sichtbarer Ausschnitt der Detailansicht in Sekunden. */
    windowSec?: number;
    color?: string;
    cues?: Cue[];
    rekordboxCues?: RekordboxCue[];
    selected?: number | null;
    height?: number;
    /** Abschnitte des Auto-Piloten als farbiges Band unten. */
    structure?: boolean;
    onpick?: (ms: number) => void;
  }

  let {
    track,
    position,
    mode = "detail",
    windowSec = 8,
    color = "#2fa8ff",
    cues = [],
    rekordboxCues = [],
    selected = null,
    height = 96,
    structure = true,
    onpick,
  }: Props = $props();

  let canvas: HTMLCanvasElement;
  let width = $state(600);
  let overviewCache: { key: string; w: number; h: number; img: HTMLCanvasElement } | null = null;

  function durationMs(t: LoadedTrack): number {
    if (t.wave) return (t.wave.columns / t.wave.perSecond) * 1000;
    return t.detail.summary.duration_s * 1000;
  }

  function cueMs(t: LoadedTrack, c: Cue): number | null {
    return c.position.type === "ms" ? c.position.ms : barToMs(t, c.position.bar, c.position.beat);
  }

  // Farben des CDJ-3000: Bass blau, Mitten orange, Überlappung braun, Höhen weiß.
  const LOW = "rgb(32,83,217)";
  const MID = "rgb(242,170,60)";
  const LOW_MID = "rgb(169,107,39)";
  const HIGH = "rgb(255,255,255)";

  /** Übersicht mit drei Bändern: von unten gestapelt wie auf dem Player. */
  function overviewBands(g: CanvasRenderingContext2D, bands: Uint8Array, columns: number, w: number, h: number) {
    let top = 1;
    for (let i = 0; i < columns; i++) top = Math.max(top, bands[i * 3] + bands[i * 3 + 1] + bands[i * 3 + 2]);
    for (let x = 0; x < w; x++) {
      const a = Math.floor((x * columns) / w);
      const b = Math.max(a + 1, Math.floor(((x + 1) * columns) / w));
      let lo = 0;
      let mi = 0;
      let hi = 0;
      for (let i = a; i < b && i < columns; i++) {
        lo = Math.max(lo, bands[i * 3]);
        mi = Math.max(mi, bands[i * 3 + 1]);
        hi = Math.max(hi, bands[i * 3 + 2]);
      }
      const k = h / top;
      g.fillStyle = HIGH;
      g.fillRect(x, h - (lo + mi + hi) * k, 1, (lo + mi + hi) * k);
      g.fillStyle = MID;
      g.fillRect(x, h - (lo + mi) * k, 1, (lo + mi) * k);
      g.fillStyle = LOW;
      g.fillRect(x, h - lo * k, 1, lo * k);
    }
  }

  /** Eine Detail-Spalte mit drei Bändern auf derselben Achse. */
  function detailBands(g: CanvasRenderingContext2D, x: number, mid: number, scale: number, lo: number, mi: number, hi: number) {
    const bar = (color: string, v: number) => {
      const bh = v * scale;
      g.fillStyle = color;
      g.fillRect(x, mid - bh, 1, bh * 2);
    };
    if (lo > mi) {
      bar(LOW, lo);
      bar(LOW_MID, mi);
    } else {
      if (lo !== mi) bar(MID, mi);
      bar(LOW_MID, lo);
    }
    bar(HIGH, hi);
  }

  function buildOverview(t: LoadedTrack, w: number, h: number): HTMLCanvasElement {
    const img = document.createElement("canvas");
    img.width = w;
    img.height = h;
    const g = img.getContext("2d")!;
    const wave = t.wave;
    if (!wave) return img;
    if (wave.bands) {
      overviewBands(g, wave.bands, wave.columns, w, h);
      return img;
    }
    for (let x = 0; x < w; x++) {
      const a = Math.floor((x * wave.columns) / w);
      const b = Math.max(a + 1, Math.floor(((x + 1) * wave.columns) / w));
      let best = a;
      for (let i = a; i < b && i < wave.columns; i++) if (wave.data[i * 4] > wave.data[best * 4]) best = i;
      const o = best * 4;
      const bh = (wave.data[o] / 31) * h;
      g.fillStyle = `rgb(${wave.data[o + 1]},${wave.data[o + 2]},${wave.data[o + 3]})`;
      g.fillRect(x, h - bh, 1, bh);
    }
    return img;
  }

  function draw() {
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    const w = Math.max(1, Math.round(width * dpr));
    const h = Math.max(1, Math.round(height * dpr));
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }
    const g = canvas.getContext("2d")!;
    g.clearRect(0, 0, w, h);
    g.fillStyle = "#07090b";
    g.fillRect(0, 0, w, h);
    const t = track;
    const pos = position();
    if (!t) {
      g.fillStyle = "#3a434e";
      g.font = `${12 * dpr}px system-ui`;
      g.fillText("keine Wellenform", 8 * dpr, h / 2 + 4 * dpr);
      return;
    }
    const dur = durationMs(t);

    if (mode === "overview") {
      if (!overviewCache || overviewCache.key !== t.detail.summary.key || overviewCache.w !== w || overviewCache.h !== h) {
        overviewCache = { key: t.detail.summary.key, w, h, img: buildOverview(t, w, h) };
      }
      g.drawImage(overviewCache.img, 0, 0);
      if (pos != null && dur > 0) {
        const x = (pos / dur) * w;
        g.fillStyle = "rgba(7,9,11,0.55)";
        g.fillRect(0, 0, x, h);
        band(g, t, w, h, dpr, (ms) => (ms / dur) * w);
        markers(g, t, w, h, dpr, (ms) => (ms / dur) * w);
        g.fillStyle = "#fff";
        g.fillRect(Math.round(x) - dpr, 0, 2 * dpr, h);
      } else {
        band(g, t, w, h, dpr, (ms) => (ms / dur) * w);
        markers(g, t, w, h, dpr, (ms) => (ms / dur) * w);
      }
      return;
    }

    // Detail: Playhead in der Mitte, Track scrollt darunter durch.
    const p = pos ?? 0;
    const msPerPx = (windowSec * 1000) / w;
    const toX = (ms: number) => w / 2 + (ms - p) / msPerPx;
    const wave = t.wave;
    const mid = h / 2;
    if (wave) {
      for (let x = 0; x < w; x++) {
        const ms = p + (x - w / 2) * msPerPx;
        if (ms < 0) continue;
        const col = Math.floor((ms / 1000) * wave.perSecond);
        if (col >= wave.columns) break;
        if (wave.bands) {
          const b = col * 3;
          detailBands(g, x, mid, (h * 0.48) / 31, wave.bands[b], wave.bands[b + 1], wave.bands[b + 2]);
          continue;
        }
        const o = col * 4;
        const bh = (wave.data[o] / 31) * (h * 0.48);
        g.fillStyle = `rgb(${wave.data[o + 1]},${wave.data[o + 2]},${wave.data[o + 3]})`;
        g.fillRect(x, mid - bh, 1, bh * 2);
      }
    }
    // Beatgrid: Taktanfänge kräftiger.
    const from = p - (w / 2) * msPerPx;
    const to = p + (w / 2) * msPerPx;
    let lo = 0;
    let hi = t.beatTimes.length;
    while (lo < hi) {
      const m = (lo + hi) >> 1;
      if (t.beatTimes[m] < from) lo = m + 1;
      else hi = m;
    }
    for (let i = lo; i < t.beatTimes.length && t.beatTimes[i] <= to; i++) {
      const x = Math.round(toX(t.beatTimes[i]));
      const down = t.beatInBar[i] === 1;
      g.fillStyle = down ? "rgba(255,80,80,0.85)" : "rgba(255,255,255,0.28)";
      g.fillRect(x, 0, down ? 2 * dpr : dpr, down ? 10 * dpr : 6 * dpr);
      g.fillRect(x, h - (down ? 10 : 6) * dpr, down ? 2 * dpr : dpr, (down ? 10 : 6) * dpr);
    }
    band(g, t, w, h, dpr, toX);
    markers(g, t, w, h, dpr, toX);
    g.fillStyle = "#fff";
    g.fillRect(Math.round(w / 2) - dpr, 0, 2 * dpr, h);
  }

  /** Struktur-Band: Intro, Groove, Breakdown, Build-up, Drop, Outro; Pausen hell. */
  function band(
    g: CanvasRenderingContext2D,
    t: LoadedTrack,
    w: number,
    h: number,
    dpr: number,
    toX: (ms: number) => number,
  ) {
    const st = t.detail.structure;
    if (!structure || !st || st.segments.length === 0) return;
    const bh = (mode === "overview" ? 4 : 5) * dpr;
    for (const p of st.pauses) {
      const a = toX(p.start_ms);
      const b = toX(p.end_ms);
      if (b < 0 || a > w) continue;
      g.fillStyle = "rgba(255,255,255,0.16)";
      g.fillRect(a, 0, Math.max(dpr, b - a), h);
    }
    for (const s of st.segments) {
      const a = Math.max(0, toX(s.start_ms));
      const b = Math.min(w, toX(s.end_ms));
      if (b <= 0 || a >= w) continue;
      g.globalAlpha = s.confidence >= 50 ? 0.95 : 0.45;
      g.fillStyle = SECTION_COLOR[s.section];
      g.fillRect(a, h - bh, Math.max(1, b - a - dpr), bh);
      g.globalAlpha = 1;
      const x0 = toX(s.start_ms);
      if (mode === "detail" && x0 >= 0 && x0 < w) {
        g.font = `bold ${9 * dpr}px system-ui`;
        g.fillText(SECTION_LABEL[s.section].toUpperCase(), x0 + 3 * dpr, h - bh - 3 * dpr);
      }
    }
  }

  function markers(
    g: CanvasRenderingContext2D,
    t: LoadedTrack,
    w: number,
    h: number,
    dpr: number,
    toX: (ms: number) => number,
  ) {
    for (const rc of rekordboxCues) {
      const x = toX(rc.position_ms);
      if (x < -20 || x > w + 20) continue;
      g.fillStyle = rc.color ? `rgb(${rc.color.join(",")})` : rc.kind.type === "memory" ? "#ff3b30" : "#3ddc84";
      g.beginPath();
      g.moveTo(x - 5 * dpr, h);
      g.lineTo(x + 5 * dpr, h);
      g.lineTo(x, h - 7 * dpr);
      g.fill();
      if (rc.kind.type === "hot") {
        g.font = `bold ${9 * dpr}px system-ui`;
        g.fillText(String.fromCharCode(64 + rc.kind.slot), x + 6 * dpr, h - 2 * dpr);
      }
    }
    for (const c of cues) {
      const ms = cueMs(t, c);
      if (ms == null) continue;
      const x = toX(ms);
      if (x < -40 || x > w + 40) continue;
      const col = c.color ? `rgb(${c.color.join(",")})` : "#ffb020";
      const sel = c.id === selected;
      // Vorlagen ohne Aktion und gesperrte Cues blass.
      const template = c.actions.length === 0;
      g.globalAlpha = c.armed && !template ? 1 : 0.4;
      g.fillStyle = col;
      g.fillRect(Math.round(x) - (sel ? 1.5 : 0.5) * dpr, 0, (sel ? 3 : 1) * dpr, h);
      g.beginPath();
      g.moveTo(x - 6 * dpr, 0);
      g.lineTo(x + 6 * dpr, 0);
      g.lineTo(x, 8 * dpr);
      g.fill();
      if ((mode === "detail" && !template) || sel) {
        g.font = `${10 * dpr}px system-ui`;
        g.fillText(c.name, x + 7 * dpr, 10 * dpr);
      }
      if (c.lead_ms > 0 && mode === "detail") {
        const lx = toX(ms - c.lead_ms);
        g.fillRect(lx, 0, x - lx, 2 * dpr);
      }
      g.globalAlpha = 1;
    }
  }

  $effect(() => {
    let raf = 0;
    const loop = () => {
      draw();
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  });

  function pick(e: MouseEvent) {
    if (!onpick || !track) return;
    const rect = canvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const dur = durationMs(track);
    let ms: number;
    if (mode === "overview") {
      ms = (x / rect.width) * dur;
    } else {
      const p = position() ?? 0;
      ms = p + (x - rect.width / 2) * ((windowSec * 1000) / rect.width);
    }
    onpick(Math.max(0, Math.min(dur, ms)));
  }
</script>

<div class="wave" bind:clientWidth={width} style:height="{height}px" style:--c={color}>
  <canvas
    bind:this={canvas}
    class:pickable={!!onpick}
    onclick={pick}
  ></canvas>
</div>

<style>
  .wave {
    width: 100%;
    position: relative;
    border-radius: 6px;
    overflow: hidden;
    border: 1px solid var(--border);
  }
  /* Absolut, damit die Leinwand die Breite nicht festhält (schmale Bildschirme). */
  canvas {
    display: block;
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }
  .pickable {
    cursor: crosshair;
  }
</style>
