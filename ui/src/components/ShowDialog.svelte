<script lang="ts">
  // Show-Verwaltung: speichern, öffnen, neu, Import/Export und Sicherungen.
  import { conn, url } from "../lib/conn.svelte";
  import type { ShowsView } from "../lib/types";
  import { toast } from "../lib/toast.svelte";

  interface Props {
    onclose: () => void;
  }
  let { onclose }: Props = $props();

  let list = $state<ShowsView | null>(null);
  let name = $state(conn.show?.name ?? "");
  let busy = $state(false);
  let fileInput: HTMLInputElement;

  async function refresh() {
    try {
      const r = await fetch(url("/api/shows"));
      list = r.ok ? ((await r.json()) as ShowsView) : null;
    } catch {
      list = null;
    }
  }
  refresh();

  async function run(cmd: Record<string, unknown>, done: string, confirmText?: string) {
    if (confirmText && !confirm(confirmText)) return;
    busy = true;
    const r = await conn.command(cmd);
    busy = false;
    if (r.ok) {
      toast(done);
      await refresh();
      if (cmd.cmd !== "show_save") onclose();
    } else {
      toast(r.error ?? "Fehler", "error");
    }
  }

  const replace = "Die laufende Show wird ersetzt. Nicht gespeicherte Änderungen bleiben nur in den Sicherungen. Weiter?";

  async function importFile(e: Event) {
    const f = (e.target as HTMLInputElement).files?.[0];
    if (!f) return;
    try {
      const file = JSON.parse(await f.text());
      await run({ cmd: "show_import", file }, `„${file.name}“ importiert`, replace);
    } catch {
      toast("Datei ist keine DeckLight-Show", "error");
    }
    fileInput.value = "";
  }

  function when(ms: number): string {
    return new Date(ms).toLocaleString("de-DE", { dateStyle: "short", timeStyle: "short" });
  }

  $effect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopImmediatePropagation();
        onclose();
      }
    };
    window.addEventListener("keydown", key, true);
    return () => window.removeEventListener("keydown", key, true);
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onclose}>
  <div class="dialog" role="dialog" tabindex="-1" aria-label="Shows" onclick={(e) => e.stopPropagation()}>
    <header>
      <h2>Shows</h2>
      <button onclick={onclose} aria-label="Schließen">✕</button>
    </header>

    <section class="row">
      <input bind:value={name} placeholder="Name der Show" disabled={conn.readonly || busy} />
      <button
        class="primary"
        disabled={conn.readonly || busy || !name.trim()}
        onclick={() => run({ cmd: "show_save", name }, `„${name.trim()}“ gespeichert`)}>Speichern</button
      >
      <button
        disabled={conn.readonly || busy || !name.trim()}
        onclick={() => run({ cmd: "show_new", name }, `Neue Show „${name.trim()}“`, replace)}>Neu</button
      >
    </section>

    <section class="row">
      <a class="button" href={url("/api/shows/export")} download>Exportieren …</a>
      <button disabled={conn.readonly || busy} onclick={() => fileInput.click()}>Importieren …</button>
      <input bind:this={fileInput} type="file" accept=".decklight,.json" hidden onchange={importFile} />
    </section>

    <h3>Gespeicherte Shows</h3>
    {#if !list}
      <p class="dim">Lade …</p>
    {:else if list.shows.length === 0}
      <p class="dim">Noch keine Show gespeichert.</p>
    {:else}
      <table>
        <tbody>
          {#each list.shows as sh (sh.file)}
            <tr class:current={sh.name === list.current}>
              <td class="name">{sh.name}</td>
              <td class="dim">{sh.tracks} Tracks · {sh.cues} Cues · {sh.triggers} Trigger</td>
              <td class="dim">{when(sh.saved_unix_ms)}</td>
              <td>
                <button
                  disabled={conn.readonly || busy}
                  onclick={() => run({ cmd: "show_open", name: sh.name }, `„${sh.name}“ geöffnet`, replace)}
                  >Öffnen</button
                >
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}

    <h3>Automatische Sicherungen</h3>
    {#if list && list.backups.length > 0}
      <div class="scroll">
        <table>
          <tbody>
            {#each list.backups as b (b.file)}
              <tr>
                <td>{when(b.saved_unix_ms)}</td>
                <td class="dim">{b.tracks} Tracks · {b.cues} Cues · {b.triggers} Trigger</td>
                <td>
                  <button
                    disabled={conn.readonly || busy}
                    onclick={() => run({ cmd: "backup_restore", file: b.file }, "Sicherung wiederhergestellt", replace)}
                    >Wiederherstellen</button
                  >
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {:else if list}
      <p class="dim">Noch keine Sicherung. DeckLight sichert die laufende Show alle fünf Minuten.</p>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 8vh;
    z-index: 50;
  }
  .dialog {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 16px 20px 20px;
    width: min(720px, calc(100vw - 32px));
    max-height: 80vh;
    overflow: auto;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  h2 {
    margin: 0;
  }
  h3 {
    font-size: 13px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-dim);
    margin: 18px 0 6px;
  }
  .row {
    display: flex;
    gap: 8px;
    margin-top: 12px;
  }
  .row input {
    flex: 1;
  }
  .button {
    display: inline-flex;
    align-items: center;
    padding: 6px 12px;
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text);
    text-decoration: none;
    font-size: 13px;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }
  td {
    padding: 6px 4px;
    border-bottom: 1px solid var(--border);
  }
  td:last-child {
    text-align: right;
  }
  .name {
    font-weight: 600;
  }
  tr.current .name {
    color: var(--accent);
  }
  .dim {
    color: var(--text-dim);
  }
  .scroll {
    max-height: 220px;
    overflow: auto;
  }
</style>
