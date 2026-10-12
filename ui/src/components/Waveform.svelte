<script lang="ts">
  // Farbige Wellenform im CDJ-3000-Stil: Übersicht (ganzer Track) oder scrollende
  // Detailansicht um den Playhead, mit Beatgrid und Cue-Markern. Zeichnet per
  // requestAnimationFrame, damit der Playhead mit 60 fps läuft.
  import type { LoadedTrack } from "../lib/tracks";
  import type { Cue, RekordboxCue } from "../lib/types";
  import { barToMs } from "../lib/tracks";

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

  function buildOverview(t: LoadedTrack, w: number, h: number): HTMLCanvasElement {
    const img = document.createElement("canvas");
    img.width = w;
    img.height = h;
    const g = img.getContext("2d")!;
    const wave = t.wave;
    if (!wave) return img;
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
        markers(g, t, w, h, dpr, (ms) => (ms / dur) * w);
        g.fillStyle = "#fff";
        g.fillRect(Math.round(x) - dpr, 0, 2 * dpr, h);
      } else {
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
    markers(g, t, w, h, dpr, toX);
    g.fillStyle = "#fff";
    g.fillRect(Math.round(w / 2) - dpr, 0, 2 * dpr, h);
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
    style:width="{width}px"
    style:height="{height}px"
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
  canvas {
    display: block;
  }
  .pickable {
    cursor: crosshair;
  }
</style>
