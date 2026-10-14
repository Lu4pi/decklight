<script lang="ts">
  // Desktop-Oberfläche im Pult-Stil: Reiter links, Werkzeuge rechts, Decks unten.
  import { conn } from "../lib/conn.svelte";
  import Live from "./Live.svelte";
  import Viewer from "./Viewer.svelte";
  import Setlist from "./Setlist.svelte";
  import Cues from "./Cues.svelte";
  import Outputs from "./Outputs.svelte";
  import Monitoring from "./Monitoring.svelte";
  import Auto from "./Auto.svelte";
  import Toast from "../components/Toast.svelte";
  import ShowDialog from "../components/ShowDialog.svelte";
  import PhoneDialog from "../components/PhoneDialog.svelte";
  import MiniDecks from "../components/MiniDecks.svelte";
  import Logo from "../components/Logo.svelte";
  import { toast } from "../lib/toast.svelte";

  const tabs = [
    ["viewer", "Viewer"],
    ["live", "Live"],
    ["setlist", "Set List"],
    ["cues", "Cue List"],
  ] as const;
  const all = ["viewer", "live", "setlist", "cues", "auto", "outputs", "monitoring"] as const;
  type Tab = (typeof all)[number];

  function fromHash(): Tab {
    const h = location.hash.slice(1);
    if (h === "decks") return "live";
    return (all.find((t) => t === h) ?? "live") as Tab;
  }
  let tab = $state<Tab>(fromHash());
  $effect(() => {
    const on = () => (tab = fromHash());
    window.addEventListener("hashchange", on);
    return () => window.removeEventListener("hashchange", on);
  });

  const s = $derived(conn.state);
  let showsOpen = $state(false);
  let phoneOpen = $state(false);
  const warnCount = $derived(conn.monitor?.warnings.filter((w) => w.level === "error").length ?? 0);

  async function panic() {
    const r = await conn.command({ cmd: "panic" });
    if (!r.ok) toast(r.error ?? "Not-Aus fehlgeschlagen", "error");
  }
  async function toggleAuto() {
    const r = await conn.command({ cmd: "set_auto", on: !s?.auto.active });
    if (!r.ok) toast(r.error ?? "Auto nicht umgeschaltet", "error");
  }
  async function arm() {
    const r = await conn.command({ cmd: "set_armed", armed: true });
    if (!r.ok) toast(r.error ?? "Fehler", "error");
  }
  // Not-Aus: Esc. Wieder scharf: Strg/Cmd + Umschalt + A.
  $effect(() => {
    const key = (e: KeyboardEvent) => {
      const typing = (e.target as HTMLElement)?.closest?.("input, textarea, select");
      if (e.key === "Escape" && !typing && !showsOpen && !phoneOpen) {
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
    <Logo size={17} />
    <button class="showname" onclick={() => (showsOpen = true)} title="Shows speichern, öffnen, sichern">
      {conn.show?.name || "Ohne Namen"}
    </button>
    <div class="leds">
      <span class="led" class:on={conn.connected} title={conn.connected ? "Verbunden" : "Keine Verbindung zum Kern"}>LINK</span>
      <span class="led" class:on={s?.timecode.enabled && s.timecode.running} title="Timecode-Ausgang">TC</span>
      <span class="led" class:on={s?.armed} class:bad={s && !s.armed} title="Trigger scharf">ARM</span>
      <span class="led" class:on={s?.auto.active && !s.auto.shadow} class:shadow={s?.auto.active && s.auto.shadow}
        title={s?.auto.active ? (s.auto.shadow ? "Auto im Schattenmodus" : "Auto-Pilot aktiv") : "Auto-Pilot aus"}>AUTO</span>
    </div>
  </header>

  <div class="bar">
    <nav>
      {#each tabs as [id, label]}
        <a href="#{id}" class:active={tab === id}>{label}</a>
      {/each}
      {#if conn.readonly}<span class="ro">nur lesend</span>{/if}
    </nav>
    <div class="tools">
      <button class="tool" onclick={() => (showsOpen = true)} title="Shows speichern, öffnen, sichern">
        <svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="10" /><circle cx="7.5" cy="12" r="1.3" class="f" /><circle cx="12" cy="12" r="1.3" class="f" /><circle cx="16.5" cy="12" r="1.3" class="f" /></svg>
        <span>Show</span>
      </button>
      <button class="tool" onclick={() => (phoneOpen = true)} title="Auf Handy oder Tablet öffnen">
        <svg viewBox="0 0 24 24"><rect x="7" y="2.5" width="10" height="19" rx="2" /><path d="M10.5 18.5h3" /></svg>
        <span>Handy</span>
      </button>
      <a class="tool" class:active={tab === "auto"} href="#auto" title="Auto-Pilot: Licht folgt Breakdown, Build-up und Drop">
        <svg viewBox="0 0 24 24"><path d="M3 17l4-6 3 3 4-8 3 6 4-3" /><path d="M3 21h18" /></svg>
        <span>Auto</span>
      </a>
      <button class="autobtn" class:on={s?.auto.active} class:shadow={s?.auto.active && s.auto.shadow} onclick={toggleAuto}
        disabled={conn.readonly} title={s?.auto.active ? "Auto-Pilot aus: Licht zurück an den Operator" : "Auto-Pilot an"}>
        AUTO<small>{s?.auto.active ? (s.auto.shadow ? "Schatten" : "an") : "aus"}</small>
      </button>
      <a class="tool" class:active={tab === "outputs"} href="#outputs" title="Netzwerk, Timecode, OSC/MIDI">
        <svg viewBox="0 0 24 24"><path d="M20 12a8 8 0 1 1-2.3-5.6M20 4v4h-4" /></svg>
        <span>Netzwerk</span>
      </a>
      <a class="tool" class:active={tab === "monitoring"} href="#monitoring" title="Geräte, Warnungen, Verlauf">
        <svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="3.2" /><path d="M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5.3 5.3l2.1 2.1M16.6 16.6l2.1 2.1M5.3 18.7l2.1-2.1M16.6 7.4l2.1-2.1" /></svg>
        <span>Utility</span>
        {#if warnCount > 0}<span class="count">{warnCount}</span>{/if}
      </a>
      {#if s?.armed}
        <button class="panic" onclick={panic} disabled={conn.readonly} title="Not-Aus (Esc)">NOT-AUS</button>
      {:else}
        <button class="arm" onclick={arm} disabled={conn.readonly} title="Wieder scharf (Strg+Umschalt+A)">
          GESPERRT<small>scharf schalten</small>
        </button>
      {/if}
    </div>
  </div>

  <main>
    {#if !conn.connected && !s}
      <p class="waiting">Warte auf den DeckLight-Kern …</p>
    {:else if tab === "live"}
      <Live />
    {:else if tab === "viewer"}
      <Viewer />
    {:else if tab === "setlist"}
      <Setlist />
    {:else if tab === "cues"}
      <Cues />
    {:else if tab === "auto"}
      <Auto />
    {:else if tab === "outputs"}
      <Outputs />
    {:else}
      <Monitoring />
    {/if}
  </main>
  {#if s && s.decks.length > 0}<MiniDecks />{/if}
  <Toast />
  {#if showsOpen}<ShowDialog onclose={() => (showsOpen = false)} />{/if}
  {#if phoneOpen}<PhoneDialog onclose={() => (phoneOpen = false)} />{/if}
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    height: 100dvh;
  }
  header {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    gap: 12px;
    padding: 6px 12px;
    background: #000;
    border-bottom: 1px solid var(--border);
  }
  .showname {
    background: none;
    border: none;
    font-weight: 700;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    font-size: 14px;
    max-width: 40vw;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .leds {
    justify-self: end;
    display: flex;
    gap: 6px;
  }
  .led {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.08em;
    padding: 2px 6px;
    border: 1px solid var(--border);
    color: var(--text-dim);
  }
  .led.on {
    background: #1fae3a;
    border-color: #1fae3a;
    color: #000;
  }
  .led.shadow {
    background: var(--accent);
    border-color: var(--accent);
    color: #000;
  }
  .autobtn {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    line-height: 1.1;
    min-width: 64px;
    padding: 4px 10px;
    border-radius: 3px;
    border: 2px solid var(--border);
    background: var(--surface-2);
    color: var(--text);
    font-weight: 800;
    letter-spacing: 0.08em;
  }
  .autobtn small {
    font-size: 9px;
    font-weight: 600;
    text-transform: uppercase;
    color: var(--text-dim);
  }
  .autobtn.on {
    background: #1fae3a;
    border-color: #1fae3a;
    color: #000;
  }
  .autobtn.on small {
    color: #000;
  }
  .autobtn.shadow {
    background: var(--accent);
    border-color: var(--accent);
  }
  .led.bad {
    background: var(--danger);
    border-color: var(--danger);
    color: #fff;
  }
  .bar {
    display: flex;
    align-items: stretch;
    justify-content: space-between;
    gap: 10px;
    padding: 4px 8px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    flex-wrap: wrap;
  }
  nav {
    display: flex;
    gap: 3px;
    align-items: stretch;
  }
  nav a {
    display: grid;
    place-items: center;
    min-width: 92px;
    padding: 10px 14px;
    color: var(--text);
    text-decoration: none;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    background: var(--surface-2);
    border: 1px solid var(--border);
  }
  nav a.active {
    background: var(--accent);
    border-color: var(--accent);
  }
  .ro {
    align-self: center;
    margin-left: 8px;
    color: var(--text-dim);
    font-size: 11px;
  }
  .tools {
    display: flex;
    gap: 4px;
    align-items: stretch;
  }
  .tool {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    min-width: 58px;
    padding: 4px 6px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: 3px;
    text-decoration: none;
    color: var(--text);
    font-size: 9px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  .tool.active {
    border-color: var(--accent);
  }
  .tool svg {
    width: 22px;
    height: 22px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
  }
  .tool svg .f {
    fill: currentColor;
    stroke: none;
  }
  .count {
    position: absolute;
    top: 2px;
    right: 2px;
    background: var(--danger);
    color: #fff;
    border-radius: 7px;
    padding: 0 5px;
    font-size: 9px;
  }
  .panic {
    background: var(--danger);
    color: #fff;
    border: none;
    font-weight: 800;
    letter-spacing: 0.08em;
    padding: 6px 16px;
    border-radius: 3px;
    font-size: 14px;
  }
  .arm {
    background: transparent;
    color: var(--danger);
    border: 2px solid var(--danger);
    font-weight: 800;
    padding: 4px 10px;
    border-radius: 3px;
    display: flex;
    flex-direction: column;
    line-height: 1.1;
    animation: blink 1s steps(2) infinite;
  }
  .arm small {
    font-size: 9px;
    font-weight: 600;
    text-transform: uppercase;
  }
  @keyframes blink {
    50% {
      opacity: 0.55;
    }
  }
  main {
    flex: 1;
    overflow: auto;
    padding: 6px 8px;
  }
  .waiting {
    color: var(--text-dim);
  }
  @media (max-width: 700px) {
    header {
      grid-template-columns: auto 1fr;
    }
    .showname {
      display: none;
    }
    nav a {
      min-width: 0;
      padding: 8px 8px;
    }
    .tool {
      min-width: 44px;
    }
  }
</style>
