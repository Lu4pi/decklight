<script lang="ts">
  // Setlist: alle bekannten Tracks mit Timecode-Offset, bearbeitbar, CSV rein/raus.
  import { conn, url } from "../lib/conn.svelte";
  import { clock, parseTimecode, toTimecode } from "../lib/format";
  import { toast } from "../lib/toast.svelte";
  import type { FrameRate } from "../lib/types";

  const rate = $derived<FrameRate>(conn.settings?.timecode.generator.rate ?? "25");

  interface Row {
    key: string;
    label: string;
    offset: number | null;
    auto: boolean;
    duration: number | null;
    bpm: number | null;
    decks: number[];
  }

  const rows = $derived.by<Row[]>(() => {
    const show = conn.show;
    if (!show) return [];
    const byKey = new Map<string, Row>();
    for (const e of show.offsets.entries) {
      byKey.set(e.key, { key: e.key, label: e.label || e.key, offset: e.offset_ms, auto: e.auto, duration: null, bpm: null, decks: [] });
    }
    for (const t of show.tracks) {
      const r = byKey.get(t.key) ?? { key: t.key, label: "", offset: null, auto: true, duration: null, bpm: null, decks: [] };
      r.label = t.artist ? `${t.artist} – ${t.title}` : t.title;
      r.duration = t.duration_s * 1000;
      r.bpm = t.bpm;
      byKey.set(t.key, r);
    }
    for (const d of conn.state?.decks ?? []) {
      if (d.info) byKey.get(d.info.key)?.decks.push(d.number);
    }
    return [...byKey.values()].sort((a, b) => (a.offset ?? Infinity) - (b.offset ?? Infinity));
  });

  let editing = $state<string | null>(null);
  let draft = $state("");

  function startEdit(r: Row) {
    if (conn.readonly) return;
    editing = r.key;
    draft = r.offset != null ? toTimecode(r.offset, rate) : "01:00:00:00";
  }

  async function commit(r: Row) {
    const ms = parseTimecode(draft, rate);
    editing = null;
    if (ms == null) {
      toast(`Ungültiger Timecode: ${draft} (Format HH:MM:SS:FF)`, "error");
      return;
    }
    const reply = await conn.command({ cmd: "set_offset", key: r.key, offset_ms: ms });
    if (!reply.ok) toast(reply.error ?? "Fehler", "error");
  }

  async function makeAuto(r: Row) {
    const reply = await conn.command({ cmd: "remove_offset", key: r.key });
    if (!reply.ok) toast(reply.error ?? "Fehler", "error");
  }

  async function importCsv(e: Event) {
    const file = (e.target as HTMLInputElement).files?.[0];
    if (!file) return;
    const csv = await file.text();
    const reply = await conn.command({ cmd: "import_offsets", csv, rate });
    if (reply.ok) toast(`${reply.count ?? 0} Offsets übernommen`);
    else toast(reply.error ?? "Import fehlgeschlagen", "error");
    (e.target as HTMLInputElement).value = "";
  }
</script>

<div class="bar">
  <h2>Setlist & Timecode-Offsets</h2>
  <span class="hint">Neue Tracks bekommen automatisch die nächste freie Stunde. Doppelklick auf einen Offset zum Ändern.</span>
  <div class="spacer"></div>
  <a class="btn" href={url("/api/offsets.csv")} download>CSV exportieren</a>
  {#if !conn.readonly}
    <label class="btn">CSV importieren<input type="file" accept=".csv,text/csv" onchange={importCsv} hidden /></label>
  {/if}
</div>

<table>
  <thead>
    <tr><th>Offset ({rate} fps)</th><th>Track</th><th>Dauer</th><th>BPM</th><th>Deck</th><th></th></tr>
  </thead>
  <tbody>
    {#each rows as r (r.key)}
      <tr class:live={r.decks.length > 0}>
        <td class="tc" ondblclick={() => startEdit(r)}>
          {#if editing === r.key}
            <!-- svelte-ignore a11y_autofocus -->
            <input
              autofocus
              bind:value={draft}
              onkeydown={(e) => {
                if (e.key === "Enter") commit(r);
                if (e.key === "Escape") editing = null;
              }}
              onblur={() => commit(r)}
            />
          {:else}
            {r.offset != null ? toTimecode(r.offset, rate) : "–"}
            {#if r.offset != null}<span class="tag" class:manual={!r.auto}>{r.auto ? "auto" : "fest"}</span>{/if}
          {/if}
        </td>
        <td class="label" title={r.key}>{r.label || r.key}</td>
        <td class="mono">{r.duration ? clock(r.duration, false) : ""}</td>
        <td class="mono">{r.bpm?.toFixed(1) ?? ""}</td>
        <td>{r.decks.join(", ")}</td>
        <td>
          {#if !conn.readonly && !r.auto && r.offset != null}
            <button class="link" onclick={() => makeAuto(r)}>wieder automatisch</button>
          {/if}
        </td>
      </tr>
    {:else}
      <tr><td colspan="6" class="dim">Noch keine Tracks. Sobald ein Player einen Track lädt, erscheint er hier.</td></tr>
    {/each}
  </tbody>
</table>

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-bottom: 12px;
    flex-wrap: wrap;
  }
  h2 {
    margin: 0;
    font-size: 18px;
  }
  .hint {
    color: var(--text-dim);
    font-size: 13px;
  }
  .spacer {
    flex: 1;
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
    padding: 7px 8px;
    border-bottom: 1px solid var(--border);
  }
  tr.live td {
    background: color-mix(in srgb, var(--accent) 8%, transparent);
  }
  .tc {
    font-family: var(--mono);
    width: 220px;
    cursor: text;
  }
  .tc input {
    font-family: var(--mono);
    width: 130px;
  }
  .tag {
    font-family: var(--font);
    font-size: 10px;
    color: var(--text-dim);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0 4px;
    margin-left: 6px;
  }
  .tag.manual {
    color: var(--accent);
    border-color: var(--accent);
  }
  .label {
    max-width: 480px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mono {
    font-family: var(--mono);
  }
  .dim {
    color: var(--text-dim);
  }
</style>
