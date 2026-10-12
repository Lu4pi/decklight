<script lang="ts">
  // Cue-Editor auf der Wellenform und globale Trigger.
  import { conn } from "../lib/conn.svelte";
  import { clock } from "../lib/format";
  import { toast } from "../lib/toast.svelte";
  import { barToMs, loadTrack, type LoadedTrack } from "../lib/tracks";
  import type { Cue, DeckFilter, GlobalEvent, GlobalTrigger, TrackSummary } from "../lib/types";
  import Waveform from "../components/Waveform.svelte";
  import CueEditor from "../components/CueEditor.svelte";
  import ActionsEditor from "../components/ActionsEditor.svelte";

  const tracks = $derived<TrackSummary[]>(conn.show?.tracks ?? []);
  const liveDeck = (key: string) => conn.state?.decks.find((d) => d.info?.key === key);

  let selectedKey = $state<string | null>(null);
  let view = $state<"track" | "globals">("track");
  let track = $state<LoadedTrack | null>(null);
  let cursor = $state(0);
  let zoom = $state(8);
  let snap = $state(true);
  let editing = $state<number | null>(null);

  // Ersten geladenen Track des Master-Decks vorwählen.
  $effect(() => {
    if (selectedKey || tracks.length === 0) return;
    const m = conn.state?.decks.find((d) => d.master && d.info?.loaded);
    selectedKey = m?.info?.key ?? tracks[0].key;
  });

  $effect(() => {
    const k = selectedKey;
    track = null;
    editing = null;
    cursor = 0;
    if (!k) return;
    let alive = true;
    loadTrack(k, true).then((t) => {
      if (alive) track = t;
    });
    return () => {
      alive = false;
    };
  });

  const cues = $derived<Cue[]>(selectedKey && conn.show ? (conn.show.triggers.tracks[selectedKey] ?? []) : []);
  const deck = $derived(selectedKey ? liveDeck(selectedKey) : undefined);
  const follow = $derived(!!deck && deck.moving);

  function position(): number | null {
    if (follow && deck) return conn.positionOf(deck.number);
    return cursor;
  }

  function cueMs(c: Cue): number | null {
    if (c.position.type === "ms") return c.position.ms;
    return track ? barToMs(track, c.position.bar, c.position.beat) : null;
  }

  function snapMs(ms: number): number {
    if (!snap || !track || track.beatTimes.length === 0) return ms;
    let best = track.beatTimes[0];
    for (const b of track.beatTimes) {
      if (Math.abs(b - ms) < Math.abs(best - ms)) best = b;
      if (b > ms + 1000) break;
    }
    return best;
  }

  function pick(ms: number) {
    cursor = snapMs(ms);
  }

  async function addCueAt(ms: number) {
    if (!selectedKey) return;
    const cue: Cue = {
      id: 0,
      name: `Cue ${cues.length + 1}`,
      position: { type: "ms", ms: Math.round(ms) },
      lead_ms: 0,
      actions: [{ type: "osc", command: "Go+ Exec 201" }],
      color: null,
      armed: true,
      scope: null,
      fire_on_landing: true,
    };
    const r = await conn.command({ cmd: "add_cue", key: selectedKey, cue });
    if (r.ok && r.id != null) editing = r.id;
    else toast(r.error ?? "Cue nicht angelegt", "error");
  }

  async function importHotCues() {
    if (!selectedKey || !track) return;
    const existing = new Set(cues.map((c) => cueMs(c)));
    let n = 0;
    for (const rc of track.detail.rekordbox_cues) {
      if (existing.has(rc.position_ms)) continue;
      const name = rc.kind.type === "hot" ? `Hot Cue ${String.fromCharCode(64 + rc.kind.slot)}` : "Memory";
      const cue: Cue = {
        id: 0,
        name: rc.comment ? `${name}: ${rc.comment}` : name,
        position: { type: "ms", ms: rc.position_ms },
        lead_ms: 0,
        actions: [],
        color: rc.color,
        armed: true,
        scope: null,
        fire_on_landing: true,
      };
      const r = await conn.command({ cmd: "add_cue", key: selectedKey, cue });
      if (r.ok) n++;
    }
    toast(n ? `${n} Rekordbox-Cues übernommen` : "Alle Rekordbox-Cues sind schon da");
  }

  async function cmd(c: Record<string, unknown>) {
    const r = await conn.command(c);
    if (!r.ok) toast(r.error ?? "Fehler", "error");
    return r;
  }

  // Globale Trigger
  const eventNames: Record<GlobalEvent, string> = {
    track_loaded: "Track geladen",
    master_changed: "Deck wird Master",
    play: "Play",
    stop: "Stop",
    on_air: "On Air",
    off_air: "Off Air",
  };
  let globalDraft = $state<GlobalTrigger | null>(null);

  function editGlobal(g: GlobalTrigger | null) {
    globalDraft = g
      ? (structuredClone($state.snapshot(g)) as GlobalTrigger)
      : { id: 0, name: "Neuer Trigger", on: "master_changed", decks: { type: "any" }, actions: [{ type: "osc", command: "Go+ Exec 10{deck}" }], armed: true };
  }

  async function saveGlobal() {
    if (!globalDraft) return;
    const g = $state.snapshot(globalDraft);
    const r = await cmd(g.id ? { cmd: "update_global", trigger: g } : { cmd: "add_global", trigger: g });
    if (r.ok) globalDraft = null;
  }

  function filterLabel(f: DeckFilter): string {
    return f.type === "any" ? "jedes Deck" : f.type === "master" ? "Master-Deck" : `Deck ${f.deck}`;
  }
</script>

<div class="layout">
  <aside>
    <button class="side" class:active={view === "globals"} onclick={() => (view = "globals")}>
      Globale Trigger <span class="n">{conn.show?.triggers.globals.length ?? 0}</span>
    </button>
    <h4>Tracks</h4>
    {#each tracks as t (t.key)}
      {@const d = liveDeck(t.key)}
      <button
        class="side"
        class:active={view === "track" && selectedKey === t.key}
        onclick={() => {
          view = "track";
          selectedKey = t.key;
        }}
      >
        <span class="tt">{t.title}</span>
        <span class="ta">{t.artist}</span>
        <span class="n">{conn.show?.triggers.tracks[t.key]?.length ?? 0}</span>
        {#if d}<span class="live">Deck {d.number}</span>{/if}
      </button>
    {:else}
      <p class="dim">Tracks erscheinen, sobald ein Player sie lädt.</p>
    {/each}

    <h4>Einstellungen</h4>
    <label class="opt">
      <select value={conn.show?.triggers.scope ?? "master"} disabled={conn.readonly}
        onchange={(e) => cmd({ cmd: "set_scope", scope: (e.target as HTMLSelectElement).value })}>
        <option value="master">Cues nur vom Master-Deck</option>
        <option value="any_deck">Cues von jedem Deck</option>
      </select>
    </label>
    <label class="opt check">
      <input type="checkbox" checked={conn.show?.triggers.import_hot_cues ?? true} disabled={conn.readonly}
        onchange={(e) => cmd({ cmd: "set_import_hot_cues", on: (e.target as HTMLInputElement).checked })} />
      Hot Cues neuer Tracks als Vorlage
    </label>
  </aside>

  <section class="content">
    {#if view === "globals"}
      <div class="head">
        <h2>Globale Trigger</h2>
        <span class="dim">Lösen bei Deck-Ereignissen aus. <code>{"{deck}"}</code> im Befehl wird durch die Decknummer ersetzt.</span>
        <div class="spacer"></div>
        {#if !conn.readonly}<button class="primary" onclick={() => editGlobal(null)}>+ Trigger</button>{/if}
      </div>
      {#if globalDraft}
        <div class="editor">
          <div class="row">
            <label>Name <input bind:value={globalDraft.name} /></label>
            <label>Wenn
              <select bind:value={globalDraft.on}>
                {#each Object.entries(eventNames) as [k, v]}<option value={k}>{v}</option>{/each}
              </select>
            </label>
            <label>Deck
              <select
                value={globalDraft.decks.type === "deck" ? String(globalDraft.decks.deck) : globalDraft.decks.type}
                onchange={(e) => {
                  const v = (e.target as HTMLSelectElement).value;
                  if (globalDraft)
                    globalDraft.decks = v === "any" ? { type: "any" } : v === "master" ? { type: "master" } : { type: "deck", deck: Number(v) };
                }}
              >
                <option value="any">jedes Deck</option>
                <option value="master">Master-Deck</option>
                {#each [1, 2, 3, 4, 5, 6] as n}<option value={String(n)}>Deck {n}</option>{/each}
              </select>
            </label>
            <label class="check"><input type="checkbox" bind:checked={globalDraft.armed} /> scharf</label>
          </div>
          <ActionsEditor bind:actions={globalDraft.actions} />
          <div class="buttons">
            <div class="spacer"></div>
            <button class="link" onclick={() => (globalDraft = null)}>Abbrechen</button>
            <button class="primary" onclick={saveGlobal}>Speichern</button>
          </div>
        </div>
      {/if}
      <table>
        <thead><tr><th>Name</th><th>Wenn</th><th>Deck</th><th>Aktionen</th><th></th></tr></thead>
        <tbody>
          {#each conn.show?.triggers.globals ?? [] as g (g.id)}
            <tr class:off={!g.armed}>
              <td>{g.name}</td>
              <td>{eventNames[g.on]}</td>
              <td>{filterLabel(g.decks)}</td>
              <td class="dim">{g.actions.length} Aktion{g.actions.length === 1 ? "" : "en"}</td>
              <td class="right">
                {#if !conn.readonly}
                  <button class="link" onclick={() => cmd({ cmd: "set_armed", id: g.id, armed: !g.armed })}>
                    {g.armed ? "sperren" : "scharf"}
                  </button>
                  <button class="link" onclick={() => cmd({ cmd: "fire", name: `Test: ${g.name}`, actions: g.actions })}>testen</button>
                  <button class="link" onclick={() => editGlobal(g)}>bearbeiten</button>
                  <button class="link danger" onclick={() => cmd({ cmd: "remove", id: g.id })}>löschen</button>
                {/if}
              </td>
            </tr>
          {:else}
            <tr><td colspan="5" class="dim">Noch keine globalen Trigger.</td></tr>
          {/each}
        </tbody>
      </table>
    {:else if selectedKey}
      {@const sum = tracks.find((t) => t.key === selectedKey)}
      <div class="head">
        <h2>{sum?.title ?? selectedKey}</h2>
        <span class="dim">{sum?.artist}</span>
        {#if deck}<span class="live">auf Deck {deck.number}{follow ? " · läuft" : ""}</span>{/if}
        <div class="spacer"></div>
        <label class="check"><input type="checkbox" bind:checked={snap} /> auf Beat einrasten</label>
        <label class="zoom">Zoom <input type="range" min="2" max="30" step="1" bind:value={zoom} /> {zoom} s</label>
      </div>

      <Waveform
        {track}
        {position}
        mode="detail"
        windowSec={zoom}
        cues={cues}
        rekordboxCues={track?.detail.rekordbox_cues ?? []}
        selected={editing}
        height={150}
        onpick={pick}
      />
      <div class="gap"></div>
      <Waveform
        {track}
        {position}
        mode="overview"
        cues={cues}
        rekordboxCues={track?.detail.rekordbox_cues ?? []}
        selected={editing}
        height={54}
        onpick={pick}
      />

      <div class="toolbar">
        <span class="mono">{clock(position())}</span>
        {#if !conn.readonly}
          <button class="primary" onclick={() => addCueAt(position() ?? 0)}>+ Cue hier</button>
          <button onclick={importHotCues} disabled={!track?.detail.rekordbox_cues.length}>Rekordbox-Cues übernehmen</button>
        {/if}
        <span class="dim">Klick in die Wellenform setzt die Position{follow ? " (folgt gerade dem Deck)" : ""}.</span>
      </div>

      <table>
        <thead><tr><th></th><th>Position</th><th>Name</th><th>Aktionen</th><th>Vorlauf</th><th></th></tr></thead>
        <tbody>
          {#each [...cues].sort((a, b) => (cueMs(a) ?? 0) - (cueMs(b) ?? 0)) as c (c.id)}
            <tr class:off={!c.armed} class:sel={editing === c.id}>
              <td><span class="swatch" style:background={c.color ? `rgb(${c.color.join(",")})` : "var(--accent)"}></span></td>
              <td class="mono">
                <button class="link" onclick={() => (cursor = cueMs(c) ?? 0)}>
                  {c.position.type === "ms" ? clock(c.position.ms) : `Takt ${c.position.bar}.${c.position.beat}`}
                </button>
              </td>
              <td>{c.name}</td>
              <td class="dim">
                {c.actions.length === 0 ? "keine (Vorlage)" : c.actions.map((a) => (a.type === "osc" ? a.command : a.type === "midi" ? `MIDI ${a.message.type}` : a.address)).join(" · ")}
              </td>
              <td class="mono">{c.lead_ms ? `${c.lead_ms} ms` : ""}</td>
              <td class="right">
                {#if !conn.readonly}
                  <button class="link" onclick={() => cmd({ cmd: "set_armed", id: c.id, armed: !c.armed })}>{c.armed ? "sperren" : "scharf"}</button>
                  <button class="link" onclick={() => cmd({ cmd: "fire_cue", id: c.id })} disabled={c.actions.length === 0}>auslösen</button>
                  <button class="link" onclick={() => (editing = editing === c.id ? null : c.id)}>bearbeiten</button>
                  <button class="link danger" onclick={() => cmd({ cmd: "remove", id: c.id })}>löschen</button>
                {/if}
              </td>
            </tr>
            {#if editing === c.id}
              <tr class="edit"><td colspan="6"><CueEditor cue={c} {track} onclose={() => (editing = null)} /></td></tr>
            {/if}
          {:else}
            <tr><td colspan="6" class="dim">Noch keine Cues. Position in der Wellenform wählen und „+ Cue hier“.</td></tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <p class="dim">Kein Track ausgewählt.</p>
    {/if}
  </section>
</div>

<style>
  .layout {
    display: grid;
    grid-template-columns: 260px minmax(0, 1fr);
    gap: 16px;
    min-height: 100%;
  }
  aside {
    display: flex;
    flex-direction: column;
    gap: 4px;
    border-right: 1px solid var(--border);
    padding-right: 12px;
  }
  h4 {
    margin: 14px 0 4px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-dim);
  }
  .side {
    display: grid;
    grid-template-columns: 1fr auto;
    text-align: left;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 6px;
    padding: 6px 8px;
    color: var(--text);
  }
  .side.active {
    background: var(--surface-2);
    border-color: var(--border);
  }
  .tt {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ta {
    grid-column: 1;
    font-size: 12px;
    color: var(--text-dim);
  }
  .n {
    grid-row: 1;
    grid-column: 2;
    font-size: 11px;
    color: var(--text-dim);
  }
  .live {
    font-size: 11px;
    color: var(--ok);
  }
  .opt {
    font-size: 13px;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
  }
  .content {
    min-width: 0;
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: 12px;
    margin-bottom: 10px;
    flex-wrap: wrap;
  }
  h2 {
    margin: 0;
    font-size: 18px;
  }
  .spacer {
    flex: 1;
  }
  .zoom {
    font-size: 12px;
    color: var(--text-dim);
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .gap {
    height: 6px;
  }
  .toolbar {
    display: flex;
    gap: 10px;
    align-items: center;
    margin: 10px 0;
    flex-wrap: wrap;
  }
  .mono {
    font-family: var(--mono);
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 14px;
  }
  th {
    text-align: left;
    color: var(--text-dim);
    font-weight: 500;
    font-size: 12px;
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
  }
  td {
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
  }
  tr.off td {
    opacity: 0.5;
  }
  tr.sel td {
    background: var(--surface-2);
  }
  tr.edit td {
    padding: 8px 0;
  }
  .right {
    text-align: right;
    white-space: nowrap;
  }
  .swatch {
    display: inline-block;
    width: 10px;
    height: 10px;
    border-radius: 2px;
  }
  .dim {
    color: var(--text-dim);
    font-size: 13px;
  }
  .editor {
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-bottom: 12px;
  }
  .editor .row {
    display: flex;
    gap: 14px;
    align-items: flex-end;
    flex-wrap: wrap;
  }
  .editor label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    color: var(--text-dim);
  }
  .editor label.check {
    flex-direction: row;
    color: var(--text);
  }
  .buttons {
    display: flex;
    gap: 8px;
  }
</style>
