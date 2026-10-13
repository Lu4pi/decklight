<script lang="ts">
  // Handy & Tablet: Adresse plus QR-Code zum Abscannen, PIN und Bedienrechte.
  import qrcode from "qrcode-generator";
  import { conn, url } from "../lib/conn.svelte";
  import { toast } from "../lib/toast.svelte";

  let { onclose }: { onclose: () => void } = $props();

  interface Access {
    port: number;
    urls: { iface: string; ip: string; monitor: string }[];
    pin: string | null;
    allow_lan_control: boolean;
  }
  let access = $state<Access | null>(null);
  let failed = $state(false);
  let pick = $state(0);

  async function refresh() {
    try {
      const r = await fetch(url("/api/access"));
      access = r.ok ? ((await r.json()) as Access) : null;
      failed = !r.ok;
    } catch {
      failed = true;
    }
  }
  refresh();
  // Einstellungen geändert (PIN, Bedienung): Adressen neu holen.
  $effect(() => {
    void conn.settings?.web.pin;
    void conn.settings?.web.allow_lan_control;
    refresh();
  });

  const target = $derived(access?.urls[Math.min(pick, access.urls.length - 1)] ?? null);

  /** QR-Code als SVG-Pfad, ein Quadrat je dunklem Modul. */
  const qr = $derived.by(() => {
    if (!target) return null;
    const q = qrcode(0, "M");
    q.addData(target.monitor);
    q.make();
    const n = q.getModuleCount();
    let d = "";
    for (let r = 0; r < n; r++) for (let c = 0; c < n; c++) if (q.isDark(r, c)) d += `M${c + 4} ${r + 4}h1v1h-1z`;
    return { size: n + 8, d };
  });

  async function setWeb(patch: Partial<{ pin: string | null; allow_lan_control: boolean }>) {
    if (!conn.settings) return;
    const settings = $state.snapshot(conn.settings);
    settings.web = { ...settings.web, ...patch };
    const r = await conn.command({ cmd: "update_settings", settings });
    if (!r.ok) toast(r.error ?? "Fehler", "error");
    else refresh();
  }
  function newPin() {
    setWeb({ pin: String(Math.floor(1000 + Math.random() * 9000)) });
  }
  async function copy() {
    if (!target) return;
    try {
      await navigator.clipboard.writeText(target.monitor);
      toast("Adresse kopiert");
    } catch {
      toast("Kopieren nicht möglich", "error");
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="dialog" role="dialog" aria-label="Handy verbinden">
    <header>
      <h2>Auf dem Handy öffnen</h2>
      <button class="icon" onclick={onclose} aria-label="Schließen">✕</button>
    </header>

    {#if failed}
      <p class="warn">Die Adresse kann nur der DeckLight-Rechner selbst anzeigen.</p>
    {:else if !access}
      <p class="dim">Lade …</p>
    {:else if access.urls.length === 0}
      <p class="warn">Dieser Rechner hat keine Netzwerkadresse. Verbinde ihn mit einem WLAN oder Netz, in dem auch das Handy ist.</p>
    {:else}
      <div class="main">
        {#if qr}
          <svg class="qr" viewBox="0 0 {qr.size} {qr.size}" shape-rendering="crispEdges" aria-label="QR-Code">
            <rect width={qr.size} height={qr.size} fill="#fff" />
            <path d={qr.d} fill="#000" />
          </svg>
        {/if}
        <div class="info">
          <ol>
            <li>Handy ins <b>gleiche WLAN</b> wie diesen Rechner bringen.</li>
            <li>QR-Code mit der Kamera scannen oder die Adresse eintippen.</li>
            <li>Im Browser <b>„Zum Home-Bildschirm“</b> wählen: DeckLight startet dann wie eine App im Vollbild.</li>
          </ol>
          <div class="url">
            <code>{target?.monitor}</code>
            <button onclick={copy}>Kopieren</button>
          </div>
          {#if access.urls.length > 1}
            <label class="cap" for="iface">Netzwerk</label>
            <select id="iface" bind:value={pick}>
              {#each access.urls as u, i}
                <option value={i}>{u.iface} · {u.ip}{u.ip.startsWith("169.254.") ? " (CDJ-Kabel)" : ""}</option>
              {/each}
            </select>
          {/if}
        </div>
      </div>

      <div class="opts">
        <div>
          <b>PIN</b>
          {#if access.pin}
            <span class="pin">{access.pin}</span> steckt im QR-Code, Fremde im Netz sehen nichts.
            <button class="link" disabled={conn.readonly} onclick={() => setWeb({ pin: null })}>PIN entfernen</button>
          {:else}
            <span class="dim">keine. Jeder im selben Netz kann zusehen.</span>
            <button class="link" disabled={conn.readonly} onclick={newPin}>PIN vergeben</button>
          {/if}
        </div>
        <label class="check">
          <input
            type="checkbox"
            checked={access.allow_lan_control}
            disabled={conn.readonly}
            onchange={(e) => setWeb({ allow_lan_control: (e.target as HTMLInputElement).checked })}
          />
          Vom Handy aus bedienen erlauben (sonst nur ansehen)
        </label>
      </div>
      <p class="hint">
        Tipp: Das CDJ-Netz bleibt am Kabel, das Handy kommt über WLAN. DeckLight hört für PRO DJ LINK nur auf die unter
        Netzwerk gewählte Schnittstelle, WLAN darf also an sein.
      </p>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.7);
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 8vh;
    z-index: 50;
  }
  .dialog {
    background: var(--surface);
    border: 1px solid var(--border);
    border-top: 3px solid var(--bar-hi);
    padding: 14px 18px 18px;
    width: min(680px, calc(100vw - 24px));
    max-height: 86vh;
    overflow: auto;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  h2 {
    margin: 0;
    font-size: 17px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  .main {
    display: flex;
    gap: 18px;
    margin-top: 14px;
    flex-wrap: wrap;
  }
  .qr {
    width: 220px;
    height: 220px;
    flex: none;
  }
  .info {
    flex: 1;
    min-width: 240px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  ol {
    margin: 0;
    padding-left: 18px;
    line-height: 1.5;
    font-size: 14px;
  }
  .url {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .url code {
    flex: 1;
    font-size: 13px;
    padding: 6px 8px;
    word-break: break-all;
  }
  .opts {
    margin-top: 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-size: 14px;
    border-top: 1px solid var(--border);
    padding-top: 12px;
  }
  .pin {
    font-family: var(--seg);
    color: var(--tc-run);
    margin: 0 6px;
  }
  .check {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .hint {
    color: var(--text-dim);
    font-size: 12px;
    margin: 12px 0 0;
  }
  .dim {
    color: var(--text-dim);
  }
  .warn {
    color: var(--tc-loop);
  }
</style>
