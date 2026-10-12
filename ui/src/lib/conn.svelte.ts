// Verbindung zum DeckLight-Kern: WebSocket-Zustand plus HTTP-Befehle.

import type { MonitorView, Reply, Settings, ShowView, Stamped, StateView } from "./types";

/** Basis-URL des Servers. In der Desktop-App kommt die Seite nicht vom Server selbst. */
export function serverBase(): string {
  const { protocol, hostname, host } = location;
  if (protocol === "http:" || protocol === "https:") {
    if (hostname !== "tauri.localhost") return `${protocol}//${host}`;
  }
  return "http://127.0.0.1:8750";
}

const pin = new URLSearchParams(location.search).get("pin");

export function url(path: string): string {
  const u = new URL(path, serverBase());
  if (pin) u.searchParams.set("pin", pin);
  return u.toString();
}

class Connection {
  connected = $state(false);
  readonly = $state(true);
  needsPin = $state(false);
  state = $state<StateView | null>(null);
  monitor = $state<MonitorView | null>(null);
  show = $state<ShowView | null>(null);
  settings = $state<Settings | null>(null);
  /** Letzte Ereignisse, neueste zuerst. */
  events = $state<Stamped[]>([]);
  /** Ankunftszeit (performance.now) des letzten Zustands, für flüssige Playheads. */
  stateAt = 0;

  private ws: WebSocket | null = null;
  private retry = 0;
  private listeners = new Set<(e: Stamped) => void>();

  start() {
    if (this.ws) return;
    const wsUrl = url("/ws").replace(/^http/, "ws");
    const ws = new WebSocket(wsUrl);
    this.ws = ws;
    ws.onopen = () => {
      this.connected = true;
      this.retry = 0;
      this.needsPin = false;
    };
    ws.onmessage = (m) => this.receive(m.data as string);
    ws.onclose = () => {
      this.connected = false;
      this.ws = null;
      this.retry = Math.min(this.retry + 1, 10);
      setTimeout(() => this.start(), 300 * this.retry);
    };
    ws.onerror = () => {
      // Ohne PIN lehnt der Server Zugriffe aus dem Netz ab (401).
      if (!pin && location.hostname !== "127.0.0.1" && location.hostname !== "localhost") {
        fetch(url("/api/state")).then((r) => (this.needsPin = r.status === 401)).catch(() => {});
      }
    };
  }

  /** Position eines Decks jetzt, zwischen den Zustands-Frames weitergerechnet. */
  positionOf(deck: number): number | null {
    const d = this.state?.decks.find((x) => x.number === deck);
    if (!d || d.position_ms == null) return null;
    if (!d.moving) return d.position_ms;
    const dt = Math.min(performance.now() - this.stateAt, 250);
    const pos = d.position_ms + dt * (1 + d.pitch_percent / 100);
    return d.track_length_ms ? Math.min(pos, d.track_length_ms) : pos;
  }

  onEvent(f: (e: Stamped) => void): () => void {
    this.listeners.add(f);
    return () => this.listeners.delete(f);
  }

  private receive(text: string) {
    const msg = JSON.parse(text);
    switch (msg.type) {
      case "hello":
        this.readonly = msg.readonly;
        if (msg.settings) this.settings = msg.settings;
        break;
      case "state":
        this.stateAt = performance.now();
        this.state = msg;
        break;
      case "monitor":
        this.monitor = msg;
        break;
      case "show":
        this.show = msg;
        break;
      case "settings":
        delete msg.type;
        this.settings = msg;
        break;
      case "event": {
        delete msg.type;
        const e = msg as Stamped;
        this.events = [e, ...this.events].slice(0, 300);
        for (const f of this.listeners) f(e);
        break;
      }
    }
  }

  async command(cmd: Record<string, unknown>): Promise<Reply> {
    try {
      const r = await fetch(url("/api/command"), {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(cmd),
      });
      const text = await r.text();
      try {
        return JSON.parse(text) as Reply;
      } catch {
        return { ok: false, error: text || `HTTP ${r.status}` };
      }
    } catch (e) {
      return { ok: false, error: String(e) };
    }
  }
}

export const conn = new Connection();
