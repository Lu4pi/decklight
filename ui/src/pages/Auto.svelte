<script lang="ts">
  // Auto-Pilot: Licht folgt dem Track, wenn der Operator kurz weg ist.
  // Oben Schalter und Live-Zustand, darunter die erkannte Struktur je Deck,
  // unten die Befehle an die grandMA3.
  import { conn } from "../lib/conn.svelte";
  import { live } from "../lib/live.svelte";
  import { toast } from "../lib/toast.svelte";
  import { SECTIONS, SECTION_COLOR, SECTION_LABEL, SOURCE_LABEL, inBars } from "../lib/sections";
  import type { AutoCommands, Section, Settings } from "../lib/types";
  import { titleLine } from "../lib/deck";
  import Waveform from "../components/Waveform.svelte";

  const s = $derived(conn.state);
  const auto = $derived(s?.auto);
  const decks = $derived(s?.decks ?? []);
  $effect(() => live.ensure(decks));

  let draft = $state<Settings | null>(null);
  let saving = $state(false);
  $effect(() => {
    if (!draft && conn.settings?.actions.auto) draft = structuredClone($state.snapshot(conn.settings)) as Settings;
  });
  const dirty = $derived(!!draft && !!conn.settings && JSON.stringify(draft.actions.auto) !== JSON.stringify(conn.settings.actions.auto));

  async function toggle() {
    const r = await conn.command({ cmd: "set_auto", on: !auto?.active });
    if (!r.ok) toast(r.error ?? "Auto nicht umgeschaltet", "error");
  }

  async function save() {
    if (!draft || !conn.settings) return;
    saving = true;
    // Nur den Auto-Teil übernehmen; der Rest bleibt, wie er gespeichert ist.
    const next = structuredClone($state.snapshot(conn.settings)) as Settings;
    next.actions.auto = $state.snapshot(draft.actions.auto) as Settings["actions"]["auto"];
    const r = await conn.command({ cmd: "update_settings", settings: next });
    saving = false;
    if (r.ok) {
      toast("Gespeichert");
      conn.settings = next;
      draft = null;
    } else toast(r.error ?? "Speichern fehlgeschlagen", "error");
  }

  /** Zeilen ↔ Liste (leere Zeilen fallen weg). */
  function lines(v: string): string[] {
    return v
      .split("\n")
      .map((l) => l.trim())
      .filter((l) => l.length > 0);
  }

  type ListKey = Exclude<keyof AutoCommands, "build_level">;
  const sectionRows: [ListKey, string, string][] = [
    ["intro", "Intro", "Anfang bis zum ersten vollen Teil"],
    ["groove", "Groove", "volle Teile ohne Breakdown davor; mehrere Zeilen wechseln sich ab"],
    ["breakdown", "Breakdown", "Bass weg"],
    ["build", "Build-up", "Spannung bis zum Drop"],
    ["drop", "Drop", "volle Teile nach einem Breakdown; mehrere Zeilen wechseln sich ab"],
    ["outro", "Outro", "Ende des Tracks"],
  ];
  const eventRows: [ListKey, string, string][] = [
    ["hit", "Drop-Hit", "zusätzlich genau auf der Eins des Drops"],
    ["pause", "Pause im Track", "Stille vor dem Drop, harter Stopp im Track"],
    ["stop", "DJ stoppt", "kein Deck läuft mehr On Air"],
    ["release", "Auto aus", "Licht zurück an den Operator"],
  ];

  function fill(cmd: string): string {
    const seq = draft?.actions.auto.sequence ?? "";
    return cmd.replaceAll("{seq}", seq).replaceAll("{pct}", "50").replaceAll("{deck}", "1").replaceAll("{bpm}", "128.0");
  }
  async function test(name: string, cmds: string[]) {
    if (cmds.length === 0) return;
    const r = await conn.command({
      cmd: "fire",
      name: `Auto-Test ${name}`,
      actions: cmds.map((c) => ({ type: "osc", command: fill(c) })),
    });
    if (!r.ok) toast(r.error ?? "Nicht gesendet", "error");
    else toast(`Gesendet: ${cmds.map(fill).join(", ")}`);
  }

  function segAt(deck: number): { section: Section; confidence: number } | null {
    const t = live.of(decks.find((d) => d.number === deck)!);
    const pos = conn.positionOf(deck);
    const st = t?.detail.structure;
    if (!st || pos == null) return null;
    const seg = st.segments.find((x) => x.start_ms <= pos && pos < x.end_ms);
    return seg ? { section: seg.section, confidence: seg.confidence } : null;
  }
</script>

<div class="auto">
  <section class="head" class:on={auto?.active} class:shadow={auto?.active && auto?.shadow}>
    <button class="switch" class:on={auto?.active} onclick={toggle} disabled={conn.readonly}>
      <b>AUTO</b>
      <span>{auto?.active ? (auto.shadow ? "Schattenmodus" : "aktiv") : "aus"}</span>
    </button>
    <div class="now">
      {#if auto?.active && auto.section}
        <div class="sec" style:--c={SECTION_COLOR[auto.section]}>
          {SECTION_LABEL[auto.section]}
          {#if auto.build_pct != null}<small>{auto.build_pct} %</small>{/if}
        </div>
        <div class="meta">
          Deck {auto.deck}
          {#if auto.source}· {SOURCE_LABEL[auto.source]} · {auto.confidence} %{/if}
          {#if auto.next}· <b style:color={SECTION_COLOR[auto.next.section]}>{SECTION_LABEL[auto.next.section]}</b> {inBars(auto.next.bars)}{/if}
        </div>
      {:else if auto?.active}
        <div class="meta">{auto.note || "Wartet auf ein laufendes Deck"}</div>
      {:else}
        <div class="meta">
          Auto übernimmt das Licht, wenn du kurz weg bist: Intro, Breakdown, Build-up und Drop erkennt DeckLight aus der
          rekordbox-Phrasenanalyse und der Wellenform und schickt die Befehle genau auf die Eins.
        </div>
      {/if}
      {#if auto?.active && auto.shadow}<div class="warn">Schattenmodus: Befehle stehen nur im Log, an die MA3 geht nichts.</div>{/if}
      {#if s && !s.armed}<div class="warn">Not-Aus aktiv: Auto sendet nichts.</div>{/if}
    </div>
  </section>

  <section>
    <h3>Erkannte Struktur</h3>
    {#if decks.filter((d) => d.info).length === 0}
      <p class="dim">Kein Track geladen.</p>
    {/if}
    {#each decks.filter((d) => d.info) as d (d.number)}
      {@const t = live.of(d)}
      {@const st = t?.detail.structure}
      {@const cur = segAt(d.number)}
      <div class="deck" class:lead={auto?.active && auto.deck === d.number}>
        <div class="dhead">
          <b>Deck {d.number}</b>
          <span class="title">{titleLine(d)}</span>
          {#if d.on_air}<span class="tag">On Air</span>{/if}
          {#if cur}<span class="tag" style:background={SECTION_COLOR[cur.section]}>{SECTION_LABEL[cur.section]}</span>{/if}
        </div>
        <Waveform track={t} position={() => conn.positionOf(d.number)} mode="overview" height={44} />
        {#if st && st.segments.length}
          <div class="segs">
            {#each st.segments as g}
              <span class="seg" style:--c={SECTION_COLOR[g.section]} class:weak={g.confidence < (draft?.actions.auto.min_confidence ?? 50)}
                title="{SOURCE_LABEL[g.source]}, Sicherheit {g.confidence} %">
                {SECTION_LABEL[g.section]} <small>{g.bars} T · {g.confidence} %</small>
              </span>
            {/each}
          </div>
          <div class="hint">
            {st.has_phrases ? "rekordbox-Phrasen" : "keine rekordbox-Phrasen"} ·
            {st.has_bands ? "Drei-Band-Wellenform" : "einfache Wellenform"} ·
            {st.pauses.length} {st.pauses.length === 1 ? "Pause" : "Pausen"}
          </div>
        {:else if t}
          <div class="hint">Keine Struktur erkannt (kein Beatgrid). Auto wechselt dann nur alle {draft?.actions.auto.rotate_bars ?? 16} Takte den Groove-Look.</div>
        {/if}
      </div>
    {/each}
    <div class="legend">
      {#each SECTIONS as sec}<span style:--c={SECTION_COLOR[sec]}>{SECTION_LABEL[sec]}</span>{/each}
    </div>
  </section>

  {#if draft}
    {@const a = draft.actions.auto}
    <div class="cols">
      <section>
        <h3>Verhalten</h3>
        <label class="check"><input type="checkbox" bind:checked={a.shadow} disabled={conn.readonly} /> Schattenmodus (nur anzeigen und loggen)</label>
        <div class="row">
          <label>Mindest-Sicherheit %
            <input type="number" min="0" max="100" bind:value={a.min_confidence} disabled={conn.readonly} />
          </label>
          <label>Vorlauf ms
            <input type="number" min="0" max="500" bind:value={a.lead_ms} disabled={conn.readonly} />
          </label>
        </div>
        <div class="row">
          <label>Look-Wechsel alle … Takte (0 = nie)
            <input type="number" min="0" max="128" bind:value={a.rotate_bars} disabled={conn.readonly} />
          </label>
          <label>Drop-Hit höchstens alle … Takte
            <input type="number" min="1" max="128" bind:value={a.hit_every_bars} disabled={conn.readonly} />
          </label>
        </div>
        <label>Tracks mit eigenen Cues
          <select bind:value={a.manual_cues} disabled={conn.readonly}>
            <option value="complement">Auto läuft trotzdem, eigene Cues kommen dazu</option>
            <option value="yield">Auto hält sich raus</option>
          </select>
        </label>
        <div class="hint">
          Auto folgt dem Deck, das On Air ist und läuft (bei mehreren dem Tempo-Master). Im Mix übernimmt es das neue Deck
          auf dessen nächster Eins, ohne Drop-Hit. Abschnitte unter der Mindest-Sicherheit zählen als Groove.
        </div>
      </section>

      <section>
        <h3>Ein/Aus von der grandMA3</h3>
        <label>OSC-Eingang (Port, leer = aus)
          <input type="number" value={a.osc_in_port ?? ""} disabled={conn.readonly}
            onchange={(e) => {
              const v = (e.target as HTMLInputElement).value.trim();
              a.osc_in_port = v === "" ? null : Number(v);
            }} />
        </label>
        <div class="hint">
          In der MA3 unter <i>In &amp; Out → OSC</i> eine Zeile mit der IP dieses Rechners und diesem Port anlegen
          (Send an). Dann auf eine Taste das Makro <code>SendOSC 2 "/decklight/auto,i,1"</code> (an) bzw.
          <code>,i,0</code> (aus) legen; ohne Wert schaltet <code>/decklight/auto</code> um. Die 2 ist die Nummer der
          OSC-Zeile.
        </div>
        <label>Sequenz für <code>{"{seq}"}</code>
          <input bind:value={a.sequence} disabled={conn.readonly} />
        </label>
        <label>Build-up stufenlos (<code>{"{pct}"}</code> = 0–100, leer = aus)
          <input bind:value={a.commands.build_level} disabled={conn.readonly} />
        </label>
      </section>
    </div>

    <section>
      <h3>Befehle je Abschnitt</h3>
      <div class="hint">
        Je Zeile ein MA3-Befehl. Bei Abschnitten wechseln mehrere Zeilen sich ab (jeder Drop nimmt den nächsten Look), bei
        Ereignissen gehen alle Zeilen raus. Platzhalter: <code>{"{seq}"}</code>, <code>{"{deck}"}</code>,
        <code>{"{bpm}"}</code>.
      </div>
      <div class="cmds">
        {#each [...sectionRows, ...eventRows] as [key, label, help]}
          <div class="cmd">
            <div class="clabel">
              {#if SECTIONS.includes(key as Section)}<i style:background={SECTION_COLOR[key as Section]}></i>{/if}
              <b>{label}</b>
              <small>{help}</small>
            </div>
            <textarea rows={Math.max(1, a.commands[key].length)} disabled={conn.readonly}
              value={a.commands[key].join("\n")}
              onchange={(e) => (a.commands[key] = lines((e.target as HTMLTextAreaElement).value))}></textarea>
            <button onclick={() => test(label, a.commands[key].slice(0, SECTIONS.includes(key as Section) ? 1 : undefined))}
              disabled={conn.readonly || dirty || a.commands[key].length === 0}>Test</button>
          </div>
        {/each}
      </div>
      {#if dirty}<div class="hint">Erst speichern, dann testen.</div>{/if}
    </section>

    {#if !conn.readonly}
      <div class="save">
        <button class="link" disabled={!dirty} onclick={() => (draft = null)}>Verwerfen</button>
        <button class="primary" disabled={!dirty || saving} onclick={save}>{saving ? "Speichere …" : "Speichern"}</button>
      </div>
    {/if}
  {/if}
</div>

<style>
  .auto {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  section {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }
  .cols {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(340px, 1fr));
    gap: 12px;
  }
  h3 {
    margin: 0;
    font-size: 15px;
  }
  .head {
    flex-direction: row;
    align-items: center;
    gap: 16px;
  }
  .head.on {
    border-color: #1fae3a;
  }
  .head.shadow {
    border-color: var(--accent);
  }
  .switch {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    min-width: 120px;
    min-height: 72px;
    border-radius: 8px;
    border: 2px solid var(--border);
    background: var(--surface-2);
    color: var(--text);
  }
  .switch b {
    font-size: 24px;
    letter-spacing: 0.12em;
  }
  .switch span {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-dim);
  }
  .switch.on {
    background: #1fae3a;
    border-color: #1fae3a;
    color: #000;
  }
  .switch.on span {
    color: #000;
  }
  .now {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .sec {
    font-size: 30px;
    font-weight: 800;
    color: var(--c);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  .sec small {
    font-size: 16px;
    margin-left: 8px;
  }
  .meta {
    color: var(--text-dim);
    font-size: 13px;
  }
  .warn {
    color: var(--accent);
    font-size: 13px;
  }
  .deck {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px;
    border: 1px solid var(--border);
    border-radius: 8px;
  }
  .deck.lead {
    border-color: #1fae3a;
  }
  .dhead {
    display: flex;
    gap: 8px;
    align-items: center;
    min-width: 0;
  }
  .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-dim);
    font-size: 13px;
  }
  .tag {
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    padding: 2px 6px;
    border-radius: 3px;
    background: var(--danger);
    color: #000;
  }
  .segs {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .seg {
    font-size: 11px;
    padding: 2px 6px;
    border-left: 4px solid var(--c);
    background: var(--surface-2);
    border-radius: 3px;
  }
  .seg small {
    color: var(--text-dim);
  }
  .seg.weak {
    opacity: 0.5;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    font-size: 11px;
    color: var(--text-dim);
  }
  .legend span::before {
    content: "";
    display: inline-block;
    width: 10px;
    height: 10px;
    margin-right: 4px;
    background: var(--c);
    vertical-align: -1px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    color: var(--text-dim);
  }
  label.check {
    flex-direction: row;
    align-items: center;
    color: var(--text);
    font-size: 14px;
  }
  .row {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
  }
  .row label {
    flex: 1;
    min-width: 140px;
  }
  .hint {
    font-size: 12px;
    color: var(--text-dim);
  }
  .dim {
    color: var(--text-dim);
  }
  .cmds {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .cmd {
    display: grid;
    grid-template-columns: minmax(160px, 240px) 1fr auto;
    gap: 8px;
    align-items: start;
  }
  .clabel {
    display: flex;
    flex-direction: column;
    font-size: 13px;
  }
  .clabel i {
    display: inline-block;
    width: 18px;
    height: 4px;
    margin-bottom: 2px;
  }
  .clabel small {
    color: var(--text-dim);
    font-size: 11px;
  }
  textarea {
    font-family: var(--mono);
    font-size: 12px;
    resize: vertical;
  }
  .save {
    position: sticky;
    bottom: 0;
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 10px;
    padding: 12px 0;
    background: var(--bg);
  }
  @media (max-width: 700px) {
    .head {
      flex-direction: column;
      align-items: stretch;
    }
    .cmd {
      grid-template-columns: 1fr;
    }
  }
</style>
