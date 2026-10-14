// Abschnitte des Auto-Piloten: Namen und Farben für Struktur-Band und Anzeigen.

import type { Section, StructureSource } from "./types";

export const SECTIONS: Section[] = ["intro", "groove", "breakdown", "build", "drop", "outro"];

export const SECTION_LABEL: Record<Section, string> = {
  intro: "Intro",
  groove: "Groove",
  breakdown: "Breakdown",
  build: "Build-up",
  drop: "Drop",
  outro: "Outro",
};

export const SECTION_COLOR: Record<Section, string> = {
  intro: "#6b7785",
  groove: "#2fbf71",
  breakdown: "#2f7bff",
  build: "#ffc21a",
  drop: "#ff3b4e",
  outro: "#9a6bff",
};

export const SOURCE_LABEL: Record<StructureSource, string> = {
  phrases: "rekordbox-Phrasen",
  waveform: "Wellenform",
  both: "Phrasen + Wellenform",
};

/** „in 4 Takten“, „im nächsten Takt“. */
export function inBars(n: number): string {
  if (n <= 1) return "im nächsten Takt";
  return `in ${n} Takten`;
}
