<script lang="ts">
  // Ausgänge und Netz: Art-Net-Timecode, MTC, OSC, MIDI, PRO-DJ-LINK-Schnittstelle, Web.
  import { conn, url } from "../lib/conn.svelte";
  import { toast } from "../lib/toast.svelte";
  import type { Settings } from "../lib/types";

  let draft = $state<Settings | null>(null);
  let interfaces = $state<{ name: string; ip: string; broadcast: string }[]>([]);
  let midiPorts = $state<string[]>([]);
  let saving = $state(false);
  let oscTest = $state("Go+ Exec 201");

  $effect(() => {
    if (!draft && conn.settings) draft = structuredClone($state.snapshot(conn.settings)) as Settings;
  });

  async function loadLists() {
    try {
      interfaces = await (await fetch(url("/api/interfaces"))).json();
      midiPorts = await (await fetch(url("/api/midi-ports"))).json();
    } catch {
      // Nur lesend oder ohne Verbindung: Listen bleiben leer.
    }
  }
  $effect(() => {
    if (conn.connected && !conn.readonly) loadLists();
  });

  const dirty = $derived(!!draft && !!conn.settings && JSON.stringify(draft) !== JSON.stringify(conn.settings));
  const restart = $derived(!!draft && !!conn.settings && JSON.stringify(draft.network) !== JSON.stringify(conn.settings.network));

  async function save() {
    if (!draft) return;
    saving = true;
    const r = await conn.command({ cmd: "update_settings", settings: $state.snapshot(draft) });
    saving = false;
    if (r.ok) {
      toast(restart ? "Gespeichert, Verbindung zu den Playern neu aufgebaut" : "Gespeichert");
      conn.settings = $state.snapshot(draft) as Settings;
      draft = null;
    } else toast(r.error ?? "Speichern fehlgeschlagen", "error");
  }

  async function test(kind: "osc" | "midi") {
    const actions =
      kind === "osc"
        ? [{ type: "osc", command: oscTest }]
        : [{ type: "midi", message: { type: "note_on", channel: 1, note: 60, velocity: 127 } }];
    await conn.command({ cmd: "fire", name: kind === "osc" ? "OSC-Test" : "MIDI-Test", actions });
    // Ergebnis kommt als Ereignis; kurz warten und letzte Meldung zeigen.
    setTimeout(() => {
      const last = conn.monitor?.log.at(-1);
      if (last?.error) toast(last.error, "error");
      else if (last) toast(`Gesendet: ${last.actions.join(", ")}`);
    }, 700);
  }

  function nullable(v: string): string | null {
    return v.trim() === "" ? null : v.trim();
  }

  const tc = $derived(conn.state?.timecode);
</script>

{#if !draft}
  <p class="dim">Einstellungen werden geladen …</p>
{:else}
  <div class="cols">
    <section>
      <h3>Timecode</h3>
      <div class="status" class:ok={tc?.running}>
        <span class="mono big">{tc?.text ?? "--:--:--:--"}</span>
        <span class="dim">{tc?.deck ? `von Deck ${tc.deck}` : "kein Master"} · {tc?.errors ?? 0} Sendefehler</span>
      </div>
      <label class="check"><input type="checkbox" bind:checked={draft.timecode_enabled} disabled={conn.readonly} /> Timecode senden</label>
      <div class="row">
        <label>Bildrate
          <select bind:value={draft.timecode.generator.rate} disabled={conn.readonly}>
            <option value="24">24 fps</option>
            <option value="25">25 fps</option>
            <option value="29.97df">29,97 fps Drop-Frame</option>
            <option value="30">30 fps</option>
          </select>
        </label>
        <label>Bei Stopp
          <select bind:value={draft.timecode.generator.stop_mode} disabled={conn.readonly}>
            <option value="freeze">letzten Wert halten</option>
            <option value="stop">verstummen</option>
          </select>
        </label>
      </div>
      <div class="row">
        <label>Art-Net an (IP oder Broadcast)
          <input placeholder="z. B. 2.255.255.255" value={draft.timecode.artnet ?? ""} disabled={conn.readonly}
            onchange={(e) => draft && (draft.timecode.artnet = nullable((e.target as HTMLInputElement).value))} />
        </label>
        <label>Port <input type="number" bind:value={draft.timecode.artnet_port} disabled={conn.readonly} /></label>
      </div>
      {#if interfaces.length}
        <div class="hint">Broadcast deiner Netze: {interfaces.map((i) => `${i.broadcast} (${i.name})`).join(", ")}</div>
      {/if}
      <label>MTC an MIDI-Ausgang
        <select value={draft.timecode.mtc_port ?? ""} disabled={conn.readonly}
          onchange={(e) => draft && (draft.timecode.mtc_port = nullable((e.target as HTMLSelectElement).value))}>
          <option value="">aus</option>
          {#each midiPorts as p}<option value={p}>{p}</option>{/each}
          {#if draft.timecode.mtc_port && !midiPorts.includes(draft.timecode.mtc_port)}
            <option value={draft.timecode.mtc_port}>{draft.timecode.mtc_port} (nicht gefunden)</option>
          {/if}
        </select>
      </label>
    </section>

    <section>
      <h3>Befehle an die grandMA3</h3>
      <div class="row">
        <label>OSC-Ziel (IP:Port)
          <input placeholder="192.168.1.50:8000" value={draft.actions.osc ?? ""} disabled={conn.readonly}
            onchange={(e) => draft && (draft.actions.osc = nullable((e.target as HTMLInputElement).value))} />
        </label>
        <label>Präfix <input placeholder="gma3" bind:value={draft.actions.osc_prefix} disabled={conn.readonly} /></label>
      </div>
      <div class="row">
        <input class="grow" bind:value={oscTest} disabled={conn.readonly} />
        <button onclick={() => test("osc")} disabled={conn.readonly || dirty}>OSC testen</button>
      </div>
      <label>MIDI-Ausgang für Note/CC
        <select value={draft.actions.midi_port ?? ""} disabled={conn.readonly}
          onchange={(e) => draft && (draft.actions.midi_port = nullable((e.target as HTMLSelectElement).value))}>
          <option value="">aus</option>
          {#each midiPorts as p}<option value={p}>{p}</option>{/each}
        </select>
      </label>
      <button onclick={() => test("midi")} disabled={conn.readonly || dirty}>MIDI testen (Note 60)</button>
      {#if dirty}<div class="hint">Erst speichern, dann testen.</div>{/if}
    </section>

    <section>
      <h3>PRO DJ LINK</h3>
      <label>Netzwerkschnittstelle
        <select value={draft.network.iface ?? ""} disabled={conn.readonly}
          onchange={(e) => draft && (draft.network.iface = nullable((e.target as HTMLSelectElement).value))}>
          <option value="">automatisch</option>
          {#each interfaces as i}<option value={i.name}>{i.name} · {i.ip}</option>{/each}
        </select>
      </label>
      <div class="hint">
        Es erscheinen nur Netzwerkkarten mit IPv4-Adresse. Fehlt die Karte zum CDJ-Netz, hat sie keinen Link (Kabel,
        Switch-LED, Adapter prüfen) oder keine Adresse: in Windows fest <code>169.254.10.10</code> /
        <code>255.255.0.0</code> eintragen.
        <button class="link" onclick={loadLists}>Liste aktualisieren</button>
      </div>
      <div class="row">
        <label>Eigene Player-Nummer
          <select bind:value={draft.network.player} disabled={conn.readonly}>
            {#each [5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15] as n}<option value={n}>{n}</option>{/each}
          </select>
        </label>
        <label class="check"><input type="checkbox" bind:checked={draft.network.passive} disabled={conn.readonly} /> nur zuhören</label>
      </div>
      <div class="hint">
        DeckLight meldet sich als virtueller Player an (nie 1–4), damit die CDJs ihren Status senden. „Nur zuhören“
        reicht nur für den Simulator.
      </div>
      <label>Rekordbox-Exporte als Rückfall (je Zeile ein Ordner mit „PIONEER“)
        <textarea rows="2" placeholder="/Volumes/USB-STICK" disabled={conn.readonly}
          value={(draft.network.usb_exports ?? []).join("\n")}
          onchange={(e) => draft && (draft.network.usb_exports = (e.target as HTMLTextAreaElement).value
            .split("\n").map((l) => l.trim()).filter((l) => l.length > 0))}></textarea>
      </label>
      <div class="hint">
        Liefert ein Player keine Titel und Wellenformen, liest DeckLight sie aus diesen Exporten
        (eingesteckter Stick oder Kopie).
      </div>
    </section>

    <section>
      <h3>Monitor & Overlay im Netz</h3>
      <label>PIN (leer = ohne)
        <input value={draft.web.pin ?? ""} disabled={conn.readonly}
          onchange={(e) => draft && (draft.web.pin = nullable((e.target as HTMLInputElement).value))} />
      </label>
      <label class="check"><input type="checkbox" bind:checked={draft.web.allow_lan_control} disabled={conn.readonly} /> Bedienung aus dem Netz erlauben</label>
      <div class="hint">
        Im Browser: <code>/monitor</code> fürs Tablet, <code>/overlay</code> für OBS (Port 8750{draft.web.pin ? ", mit ?pin=…" : ""}).
      </div>
    </section>
  </div>

  {#if !conn.readonly}
    <div class="save">
      {#if restart}<span class="warn">Änderungen am Netz bauen die Verbindung zu den Playern neu auf.</span>{/if}
      <button class="link" disabled={!dirty} onclick={() => (draft = null)}>Verwerfen</button>
      <button class="primary" disabled={!dirty || saving} onclick={save}>{saving ? "Speichere …" : "Speichern"}</button>
    </div>
  {/if}
{/if}

<style>
  .cols {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(360px, 1fr));
    gap: 14px;
  }
  section {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  h3 {
    margin: 0;
    font-size: 15px;
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
    align-items: flex-end;
    flex-wrap: wrap;
  }
  .grow {
    flex: 1;
  }
  .hint {
    font-size: 12px;
    color: var(--text-dim);
  }
  .status {
    display: flex;
    flex-direction: column;
  }
  .big {
    font-size: 30px;
    font-weight: 700;
    color: var(--text-dim);
  }
  .status.ok .big {
    color: var(--accent);
  }
  .mono {
    font-family: var(--mono);
  }
  .dim {
    color: var(--text-dim);
    font-size: 13px;
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
  .warn {
    color: var(--accent);
    font-size: 13px;
  }
</style>
