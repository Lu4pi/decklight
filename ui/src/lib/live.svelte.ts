// Geladene Tracks der Decks, geteilt von allen Live-Ansichten.

import { loadTrack, type LoadedTrack } from "./tracks";
import type { DeckView } from "./types";

class LiveTracks {
  map = $state<Record<string, LoadedTrack | null>>({});
  /** Stand der Wellenform je Track, um nachgelieferte Analysen neu zu laden. */
  private waves: Record<string, number> = {};

  /** In einem `$effect` aufrufen: lädt fehlende Tracks der Decks nach. */
  ensure(decks: DeckView[]) {
    for (const d of decks) {
      const k = d.info?.loaded ? d.info.key : null;
      const wave = d.info?.wave ?? 0;
      const stale = k !== null && k in this.map && this.waves[k] !== wave;
      if (k && (!(k in this.map) || stale)) {
        this.waves[k] = wave;
        if (!stale) this.map[k] = null;
        loadTrack(k, stale).then((t) => {
          if (t) this.map[k] = t;
          else delete this.map[k];
        });
      }
    }
  }

  of(d: DeckView | undefined | null): LoadedTrack | null {
    return d?.info ? (this.map[d.info.key] ?? null) : null;
  }
}

export const live = new LiveTracks();
