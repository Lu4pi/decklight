<script lang="ts">
  // Desktop-Oberfläche: Kopfzeile mit Timecode und Not-Aus, Reiter für die Bereiche.
  import { conn } from "../lib/conn.svelte";
  import type { MasterMode } from "../lib/types";
  import Decks from "./Decks.svelte";
  import Setlist from "./Setlist.svelte";
  import Cues from "./Cues.svelte";
  import Outputs from "./Outputs.svelte";
  import Monitoring from "./Monitoring.svelte";
  import Toast from "../components/Toast.svelte";
  import ShowDialog from "../components/ShowDialog.svelte";
  import { toast } from "../lib/toast.svelte";

  const tabs = [
    ["decks", "Decks"],
    ["setlist", "Setlist & Offsets"],
    ["cues", "Cues & Trigger"],
    ["outputs", "Ausgänge"],
    ["monitoring", "Monitoring"],
  ] as const;
  type Tab = (typeof tabs)[number][0];

  function fromHash(): Tab {
    const h = location.hash.slice(1);
    return (tabs.find((t) => t[0] === h)?.[0] ?? "decks") as Tab;
  }
  let tab = $state<Tab>(fromHash());
  $effect(() => {
    const on = () => (tab = fromHash());
    window.addEventListener("hashchange", on);
    return () => window.removeEventListener("hashchange", on);
  });

  const s = $derived(conn.state);
  let showsOpen = $state(false);
  const warnCount = $derived(conn.monitor?.warnings.filter((w) => w.level === "error").length ?? 0);

  async function panic() {
    const r = await conn.command({ cmd: "panic" });
    if (!r.ok) toast(r.error ?? "Not-Aus fehlgeschlagen", "error");
  }
  async function arm() {
    const r = await conn.command({ cmd: "set_armed", armed: true });
    if (!r.ok) toast(r.error ?? "Fehler", "error");
  }
  async function setMaster(e: Event) {
    const v = (e.target as HTMLSelectElement).value;
    const mode: MasterMode =
      v === "on_air" ? { mode: "on_air" } : v === "tempo_master" ? { mode: "tempo_master" } : { mode: "manual", deck: Number(v) };
    const r = await conn.command({ cmd: "set_master_mode", mode });
    if (!r.ok) toast(r.error ?? "Fehler", "error");
  }
  const masterValue = $derived(
    !s ? "on_air" : s.master_mode.mode === "manual" ? String(s.master_mode.deck) : s.master_mode.mode,
  );

  // Not-Aus: Esc. Wieder scharf: Strg/Cmd + Umschalt + A.
  $effect(() => {
    const key = (e: KeyboardEvent) => {
      const typing = (e.target as HTMLElement)?.closest?.("input, textarea, select");
      if (e.key === "Escape" && !typing && !showsOpen) {
        e.preventDefault();
        panic();
      } else if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key.toLowerCase() === "a") {
        e.preventDefault();
        arm();
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  });
</script>

<div class="app">
  <header>
    <div class="brand">
      <span class="logo">◐</span> DeckLight
      <span class="dot" class:on={conn.connected} title={conn.connected ? "Verbunden" : "Keine Verbindung zum Kern"}></span>
    </div>

    <button class="showname" onclick={() => (showsOpen = true)} title="Shows speichern, öffnen, sichern">
      <span class="label">Show</span>
      {conn.show?.name || "Ohne Namen"} ▾
    </button>

    <div class="tc" class:running={s?.timecode.running} title="Timecode-Ausgang">
      <span class="tcval">{s?.timecode.enabled ? (s?.timecode.text ?? "--:--:--:--") : "TC aus"}</span>
      <span class="tcinfo">{s?.timecode.deck ? `Deck ${s.timecode.deck}` : "kein Master"}</span>
    </div>

    <label class="master">
      Master
      <select value={masterValue} onchange={setMaster} disabled={conn.readonly}>
        <option value="on_air">On Air / Fader</option>
        <option value="tempo_master">Tempo-Master</option>
        {#each s?.decks ?? [] as d}
          <option value={String(d.number)}>Deck {d.number} fest</option>
        {/each}
      </select>
    </label>

    <div class="spacer"></div>

    {#if s?.armed}
      <button class="panic" onclick={panic} disabled={conn.readonly} title="Not-Aus (Esc)">NOT-AUS</button>
    {:else}
      <button class="arm" onclick={arm} disabled={conn.readonly} title="Wieder scharf (Strg+Umschalt+A)">
        GESPERRT · scharf schalten
      </button>
    {/if}
  </header>

  <nav>
    {#each tabs as [id, label]}
      <a href="#{id}" class:active={tab === id}>
        {label}
        {#if id === "monitoring" && warnCount > 0}<span class="count">{warnCount}</span>{/if}
      </a>
    {/each}
    {#if conn.readonly}<span class="ro">nur lesend</span>{/if}
  </nav>

  <main>
    {#if !conn.connected && !s}
      <p class="waiting">Warte auf den DeckLight-Kern …</p>
    {:else if tab === "decks"}
      <Decks />
    {:else if tab === "setlist"}
      <Setlist />
    {:else if tab === "cues"}
      <Cues />
    {:else if tab === "outputs"}
      <Outputs />
    {:else}
      <Monitoring />
    {/if}
  </main>
  <Toast />
  {#if showsOpen}<ShowDialog onclose={() => (showsOpen = false)} />{/if}
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  header {
    display: flex;
    align-items: center;
    gap: 18px;
    padding: 10px 16px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }
  .brand {
    font-weight: 800;
    font-size: 18px;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .logo {
    color: var(--accent);
  }
  .dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--danger);
  }
  .dot.on {
    background: var(--ok);
  }
  .showname {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    line-height: 1.15;
    background: transparent;
    border: 1px solid var(--border);
    padding: 4px 10px;
    font-weight: 600;
    max-width: 220px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .showname .label {
    font-size: 10px;
    color: var(--text-dim);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-weight: 400;
  }
  .tc {
    display: flex;
    flex-direction: column;
    line-height: 1.1;
  }
  .tcval {
    font-family: var(--mono);
    font-size: 26px;
    font-weight: 700;
    color: var(--text-dim);
    letter-spacing: 0.04em;
  }
  .tc.running .tcval {
    color: var(--accent);
  }
  .tcinfo {
    font-size: 11px;
    color: var(--text-dim);
  }
  .master {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-dim);
    font-size: 13px;
  }
  .spacer {
    flex: 1;
  }
  .panic {
    background: var(--danger);
    color: #fff;
    border: none;
    font-weight: 800;
    letter-spacing: 0.08em;
    padding: 10px 22px;
    border-radius: 8px;
    font-size: 15px;
  }
  .arm {
    background: transparent;
    color: var(--danger);
    border: 2px solid var(--danger);
    font-weight: 700;
    padding: 8px 16px;
    border-radius: 8px;
    animation: blink 1s steps(2) infinite;
  }
  @keyframes blink {
    50% {
      opacity: 0.55;
    }
  }
  nav {
    display: flex;
    gap: 2px;
    padding: 0 12px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    align-items: center;
  }
  nav a {
    padding: 10px 14px;
    color: var(--text-dim);
    text-decoration: none;
    border-bottom: 2px solid transparent;
    font-size: 14px;
  }
  nav a.active {
    color: var(--text);
    border-bottom-color: var(--accent);
  }
  .count {
    background: var(--danger);
    color: #fff;
    border-radius: 8px;
    padding: 0 6px;
    font-size: 11px;
    margin-left: 4px;
  }
  .ro {
    margin-left: auto;
    color: var(--text-dim);
    font-size: 12px;
  }
  main {
    flex: 1;
    overflow: auto;
    padding: 14px 16px;
  }
  .waiting {
    color: var(--text-dim);
  }
</style>
