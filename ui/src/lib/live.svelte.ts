// Geladene Tracks der Decks, geteilt von allen Live-Ansichten.

import { loadTrack, type LoadedTrack } from "./tracks";
import type { DeckView } from "./types";

class LiveTracks {
  map = $state<Record<string, LoadedTrack | null>>({});

  /** In einem `$effect` aufrufen: lädt fehlende Tracks der Decks nach. */
  ensure(decks: DeckView[]) {
    for (const d of decks) {
      const k = d.info?.loaded ? d.info.key : null;
      if (k && !(k in this.map)) {
        this.map[k] = null;
        loadTrack(k).then((t) => {
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
