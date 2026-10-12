// Spiegel der JSON-Strukturen des DeckLight-Servers (crates/server/src/view.rs).

export type PlayState =
  | "no_track"
  | "loading"
  | "playing"
  | "looping"
  | "paused"
  | "cued"
  | "cue_playing"
  | "cue_scratch"
  | "searching"
  | "spun_down"
  | "ended"
  | { unknown: number };

export type MasterMode = { mode: "on_air" } | { mode: "tempo_master" } | { mode: "manual"; deck: number };

export interface TrackRef {
  source_player: number;
  slot: string;
  rekordbox_id: number;
}

export interface TrackSummary {
  key: string;
  title: string;
  artist: string;
  album: string;
  musical_key: string;
  duration_s: number;
  bpm: number | null;
  has_artwork: boolean;
  loaded: boolean;
  offset_ms: number | null;
}

export interface DeckView {
  number: number;
  name: string;
  track: TrackRef | null;
  play_state: PlayState;
  moving: boolean;
  position_ms: number | null;
  track_length_ms: number | null;
  pitch_percent: number;
  track_bpm: number | null;
  effective_bpm: number | null;
  beat_in_bar: number;
  on_air: boolean;
  tempo_master: boolean;
  precise: boolean;
  master: boolean;
  info: TrackSummary | null;
  timecode_ms: number | null;
}

export interface StateView {
  t: number;
  master: number | null;
  master_mode: MasterMode;
  armed: boolean;
  decks: DeckView[];
  timecode: { text: string | null; deck: number | null; running: boolean; errors: number; enabled: boolean };
  mixer_bpm: number | null;
}

export interface DeviceInfo {
  number: number;
  name: string;
  kind: "player" | "mixer" | "rekordbox" | "other";
  ip: string;
}

export interface Warning {
  unix_ms: number;
  level: "info" | "warning" | "error";
  text: string;
}

export interface HistoryEntry {
  unix_ms: number;
  deck: number;
  key: string;
  title: string;
  artist: string;
}

export interface LogEntry {
  t: number;
  unix_ms: number;
  deck: number | null;
  source: string;
  actions: string[];
  late_ms: number;
  error: string | null;
}

export interface MonitorView {
  devices: DeviceInfo[];
  warnings: Warning[];
  history: HistoryEntry[];
  log: LogEntry[];
}

export type OscArg = { type: "int"; value: number } | { type: "float"; value: number } | { type: "string"; value: string };

export type MidiMessage =
  | { type: "note_on"; channel: number; note: number; velocity: number }
  | { type: "note_off"; channel: number; note: number }
  | { type: "control_change"; channel: number; controller: number; value: number }
  | { type: "program_change"; channel: number; program: number };

export type Action =
  | { type: "osc"; command: string }
  | { type: "osc_message"; address: string; args: OscArg[] }
  | { type: "midi"; message: MidiMessage };

export type CuePosition = { type: "ms"; ms: number } | { type: "bar"; bar: number; beat: number };
export type FireScope = "master" | "any_deck";

export interface Cue {
  id: number;
  name: string;
  position: CuePosition;
  lead_ms: number;
  actions: Action[];
  color: [number, number, number] | null;
  armed: boolean;
  scope: FireScope | null;
  fire_on_landing: boolean;
}

export type GlobalEvent = "track_loaded" | "master_changed" | "play" | "stop" | "on_air" | "off_air";
export type DeckFilter = { type: "any" } | { type: "master" } | { type: "deck"; deck: number };

export interface GlobalTrigger {
  id: number;
  name: string;
  on: GlobalEvent;
  decks: DeckFilter;
  actions: Action[];
  armed: boolean;
}

export interface TriggerTable {
  armed: boolean;
  scope: FireScope;
  import_hot_cues: boolean;
  tracks: Record<string, Cue[]>;
  globals: GlobalTrigger[];
}

export interface OffsetEntry {
  key: string;
  label: string;
  offset_ms: number;
  auto: boolean;
}

export interface ShowView {
  name: string;
  offsets: { rule: { first_hour: number; step_hours: number }; entries: OffsetEntry[] };
  triggers: TriggerTable;
  tracks: TrackSummary[];
}

export type FrameRate = "24" | "25" | "29.97df" | "30";

export interface Settings {
  network: { iface: string | null; player: number; passive: boolean; port_base: number; db_port: number };
  master_mode: MasterMode;
  timecode_enabled: boolean;
  timecode: {
    generator: { rate: FrameRate; stop_mode: "freeze" | "stop" };
    artnet: string | null;
    artnet_port: number;
    mtc_port: string | null;
  };
  actions: { osc: string | null; osc_prefix: string; midi_port: string | null };
  web: { pin: string | null; allow_lan_control: boolean };
}

export interface RekordboxCue {
  kind: { type: "memory" } | { type: "hot"; slot: number };
  position_ms: number;
  loop_end_ms: number | null;
  color: [number, number, number] | null;
  comment: string;
}

export interface TrackDetail {
  summary: TrackSummary;
  beats: [number, number][];
  rekordbox_cues: RekordboxCue[];
  cues: Cue[];
  waveform: { per_second: number; columns: number; data: string } | null;
}

export interface Stamped {
  t: number;
  event: string;
  [k: string]: unknown;
}

export interface Reply {
  ok: boolean;
  id?: number;
  error?: string;
  count?: number;
}

export interface ShowInfo {
  name: string;
  file: string;
  saved_unix_ms: number;
  tracks: number;
  cues: number;
  triggers: number;
}

export interface ShowsView {
  current: string;
  shows: ShowInfo[];
  backups: ShowInfo[];
}
