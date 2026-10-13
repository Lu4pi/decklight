<script lang="ts">
  // Geräte, Warnungen, Aktions-Log und Track-History.
  import { conn, url } from "../lib/conn.svelte";
  import { time } from "../lib/format";
  import { toast } from "../lib/toast.svelte";

  const m = $derived(conn.monitor);
  const kinds: Record<string, string> = { player: "Player", mixer: "Mixer", rekordbox: "rekordbox", other: "Gerät" };

  // Player im Netz, aber kein Status an uns: DeckLight ist bei ihnen nicht angemeldet.
  const noStatus = $derived(
    !!m && m.devices.some((d) => d.kind === "player") && m.link.packets[2] === 0 && !conn.state?.decks.length,
  );
  let fixing = $state(false);
  async function fixFirewall() {
    fixing = true;
    const r = await conn.command({ cmd: "fix_firewall" });
    fixing = false;
    if (r.ok) toast("Firewall freigegeben");
    else toast(r.error ?? "Fehler", "error");
  }

  async function clearHistory() {
    const r = await conn.command({ cmd: "clear_history" });
    if (!r.ok) toast(r.error ?? "Fehler", "error");
  }
</script>

{#if !m}
  <p class="dim">Warte auf Daten …</p>
{:else}
  <div class="cols">
    <section>
      <h3>Geräte im Netz <span class="n">{m.devices.length}</span></h3>
      <table>
        <thead><tr><th>#</th><th>Name</th><th>Typ</th><th>IP</th></tr></thead>
        <tbody>
          {#each m.devices as d (d.number)}
            <tr><td class="mono">{d.number}</td><td>{d.name}</td><td>{kinds[d.kind] ?? d.kind}</td><td class="mono">{d.ip}</td></tr>
          {:else}
            <tr><td colspan="4" class="dim">Keine Geräte. Kabel und Netzwerkschnittstelle prüfen.</td></tr>
          {/each}
        </tbody>
      </table>
      <div class="facts">
        <span>DeckLight: <b>{m.link.ip ? `#${m.link.number} · ${m.link.ip}` : "nur zuhören"}</b></span>
        <span title="Empfangene Pakete: Ankündigung (50000) · Beat (50001) · Status (50002)">
          Pakete: <b class="mono">{m.link.packets.join(" · ")}</b>
        </span>
        {#if m.link.firewall_ok != null}
          <span>
            Firewall: <b class:bad={!m.link.firewall_ok}>{m.link.firewall_ok ? "freigegeben" : "nicht freigegeben"}</b>
            {#if !m.link.firewall_ok}
              <button class="small" disabled={conn.readonly || fixing} onclick={fixFirewall}>Firewall freigeben</button>
            {/if}
          </span>
        {/if}
        <a class="btn small" href={url("/api/log.txt")} download>Log-Datei</a>
      </div>
      {#if noStatus}
        <p class="hint">
          Die Player schicken DeckLight keinen Status, deshalb erscheinen keine Decks. Unter Netzwerk die Netzwerkkarte zum
          CDJ-Netz wählen. Fehlt sie dort, hat sie keine IP: unter Windows fest <code>169.254.10.10</code> /
          <code>255.255.0.0</code> eintragen.
        </p>
      {/if}
      {#if conn.state}
        <div class="facts">
          <span>Master: <b>{conn.state.master ?? "–"}</b></span>
          <span>Mixer-BPM: <b>{conn.state.mixer_bpm?.toFixed(1) ?? "–"}</b></span>
          <span>Trigger: <b class:bad={!conn.state.armed}>{conn.state.armed ? "scharf" : "gesperrt"}</b></span>
          <span>Timecode-Fehler: <b class:bad={conn.state.timecode.errors > 0}>{conn.state.timecode.errors}</b></span>
        </div>
      {/if}
    </section>

    <section>
      <h3>Warnungen</h3>
      <ul class="list">
        {#each [...m.warnings].reverse() as w}
          <li class={w.level}><span class="t">{time(w.unix_ms)}</span>{w.text}</li>
        {:else}
          <li class="dim">Alles ruhig.</li>
        {/each}
      </ul>
    </section>

    <section>
      <h3>
        Aktions-Log
        <a class="btn small" href={url("/api/log.csv")} download>CSV</a>
      </h3>
      <table>
        <thead><tr><th>Zeit</th><th>Quelle</th><th>Deck</th><th>Aktionen</th><th>Verzug</th></tr></thead>
        <tbody>
          {#each [...m.log].reverse() as e}
            <tr class:err={e.error}>
              <td class="mono">{time(e.unix_ms)}</td>
              <td>{e.source}</td>
              <td>{e.deck ?? ""}</td>
              <td class="dim">{e.actions.join(", ")}{#if e.error}<div class="error">{e.error}</div>{/if}</td>
              <td class="mono">{e.late_ms.toFixed(1)} ms</td>
            </tr>
          {:else}
            <tr><td colspan="5" class="dim">Noch nichts ausgelöst.</td></tr>
          {/each}
        </tbody>
      </table>
    </section>

    <section>
      <h3>
        Track-History
        <a class="btn small" href={url("/api/history.csv")} download>CSV</a>
        {#if !conn.readonly}<button class="link small" onclick={clearHistory}>leeren</button>{/if}
      </h3>
      <table>
        <thead><tr><th>Zeit</th><th>Deck</th><th>Artist</th><th>Titel</th></tr></thead>
        <tbody>
          {#each [...m.history].reverse() as h}
            <tr><td class="mono">{time(h.unix_ms)}</td><td>{h.deck}</td><td>{h.artist}</td><td>{h.title}</td></tr>
          {:else}
            <tr><td colspan="4" class="dim">Noch kein Track gespielt.</td></tr>
          {/each}
        </tbody>
      </table>
    </section>
  </div>
{/if}

<style>
  .cols {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(440px, 1fr));
    gap: 14px;
  }
  section {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 14px;
    max-height: 46vh;
    overflow: auto;
  }
  h3 {
    margin: 0 0 8px;
    font-size: 15px;
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .n {
    color: var(--text-dim);
    font-weight: 400;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }
  th {
    text-align: left;
    color: var(--text-dim);
    font-weight: 500;
    font-size: 12px;
    padding: 4px 6px;
    border-bottom: 1px solid var(--border);
  }
  td {
    padding: 5px 6px;
    border-bottom: 1px solid var(--border);
    vertical-align: top;
  }
  tr.err td:first-child {
    border-left: 2px solid var(--danger);
  }
  .error {
    color: var(--danger);
  }
  .mono {
    font-family: var(--mono);
  }
  .dim {
    color: var(--text-dim);
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    font-size: 13px;
  }
  .list li {
    padding: 4px 0;
    border-bottom: 1px solid var(--border);
  }
  .list .t {
    font-family: var(--mono);
    color: var(--text-dim);
    margin-right: 10px;
  }
  .list .warning {
    color: var(--accent);
  }
  .list .error {
    color: var(--danger);
  }
  .facts {
    display: flex;
    gap: 16px;
    flex-wrap: wrap;
    margin-top: 10px;
    font-size: 13px;
    color: var(--text-dim);
  }
  .facts b {
    color: var(--text);
  }
  .facts b.bad {
    color: var(--danger);
  }
  .small {
    font-size: 12px;
  }
  .hint {
    color: var(--tc-loop);
    font-size: 13px;
    margin: 8px 0 0;
  }
  button.small {
    font-size: 11px;
    padding: 1px 8px;
    margin-left: 6px;
  }
</style>
