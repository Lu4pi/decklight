<script lang="ts">
  // Einen Cue bearbeiten: Name, Position (Zeit oder Takt), Vorlauf, Farbe, Aktionen.
  import { conn } from "../lib/conn.svelte";
  import { clock, parseClock } from "../lib/format";
  import { toast } from "../lib/toast.svelte";
  import { barToMs, msToBar, type LoadedTrack } from "../lib/tracks";
  import type { Cue } from "../lib/types";
  import ActionsEditor from "./ActionsEditor.svelte";

  let { cue, track, onclose }: { cue: Cue; track: LoadedTrack | null; onclose: () => void } = $props();

  // svelte-ignore state_referenced_locally
  let draft = $state<Cue>(structuredClone($state.snapshot(cue)) as Cue);
  let posText = $state(draft.position.type === "ms" ? clock(draft.position.ms) : "");
  const readonly = $derived(conn.readonly);

  function hex(c: [number, number, number] | null): string {
    const v = c ?? [255, 176, 32];
    return "#" + v.map((x) => x.toString(16).padStart(2, "0")).join("");
  }
  function fromHex(h: string): [number, number, number] {
    return [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16)) as [number, number, number];
  }

  function setMode(mode: "ms" | "bar") {
    if (mode === draft.position.type) return;
    if (mode === "bar") {
      const ms = draft.position.type === "ms" ? draft.position.ms : 0;
      const b = track ? msToBar(track, ms) : null;
      draft.position = { type: "bar", bar: b?.bar ?? 1, beat: b?.beat ?? 1 };
    } else {
      const ms = draft.position.type === "bar" && track ? barToMs(track, draft.position.bar, draft.position.beat) : 0;
      draft.position = { type: "ms", ms: ms ?? 0 };
      posText = clock(ms ?? 0);
    }
  }

  async function save() {
    if (draft.position.type === "ms") {
      const ms = parseClock(posText);
      if (ms == null) {
        toast("Position: Minuten:Sekunden, z. B. 1:23.5", "error");
        return;
      }
      draft.position = { type: "ms", ms };
    }
    const r = await conn.command({ cmd: "update_cue", cue: $state.snapshot(draft) });
    if (r.ok) onclose();
    else toast(r.error ?? "Speichern fehlgeschlagen", "error");
  }

  async function test() {
    const r = await conn.command({ cmd: "fire", name: `Test: ${draft.name}`, actions: $state.snapshot(draft.actions) });
    if (!r.ok) toast(r.error ?? "Fehler", "error");
  }
</script>

<div class="editor">
  <div class="grid">
    <label>Name <input bind:value={draft.name} disabled={readonly} /></label>
    <label>
      Position
      <span class="pos">
        <select value={draft.position.type} disabled={readonly}
          onchange={(e) => setMode((e.target as HTMLSelectElement).value as "ms" | "bar")}>
          <option value="ms">Zeit</option>
          <option value="bar">Takt/Schlag</option>
        </select>
        {#if draft.position.type === "ms"}
          <input class="mono" bind:value={posText} disabled={readonly} placeholder="1:23.5" />
        {:else}
          <input type="number" min="1" bind:value={draft.position.bar} disabled={readonly} title="Takt" />
          <input type="number" min="1" max="4" bind:value={draft.position.beat} disabled={readonly} title="Schlag" />
        {/if}
      </span>
    </label>
    <label>Vorlauf (ms) <input type="number" min="0" step="10" bind:value={draft.lead_ms} disabled={readonly} /></label>
    <label>Farbe
      <input type="color" value={hex(draft.color)} disabled={readonly}
        onchange={(e) => (draft.color = fromHex((e.target as HTMLInputElement).value))} />
    </label>
    <label>Auslösen
      <select value={draft.scope ?? "default"} disabled={readonly}
        onchange={(e) => {
          const v = (e.target as HTMLSelectElement).value;
          draft.scope = v === "default" ? null : (v as "master" | "any_deck");
        }}>
        <option value="default">wie global</option>
        <option value="master">nur Master-Deck</option>
        <option value="any_deck">jedes Deck</option>
      </select>
    </label>
    <label class="check"><input type="checkbox" bind:checked={draft.armed} disabled={readonly} /> scharf</label>
    <label class="check" title="Auch auslösen, wenn ein Hot Cue genau auf diesen Cue springt">
      <input type="checkbox" bind:checked={draft.fire_on_landing} disabled={readonly} /> bei Sprung darauf
    </label>
  </div>
  <ActionsEditor bind:actions={draft.actions} disabled={readonly} />
  <div class="buttons">
    <button onclick={test} disabled={readonly || draft.actions.length === 0}>Testen</button>
    <div class="spacer"></div>
    <button class="link" onclick={onclose}>Abbrechen</button>
    <button class="primary" onclick={save} disabled={readonly}>Speichern</button>
  </div>
</div>

<style>
  .editor {
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .grid {
    display: flex;
    flex-wrap: wrap;
    gap: 12px 18px;
    align-items: flex-end;
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
  }
  .pos {
    display: flex;
    gap: 4px;
  }
  .pos input[type="number"] {
    width: 64px;
  }
  .mono {
    font-family: var(--mono);
    width: 100px;
  }
  .buttons {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .spacer {
    flex: 1;
  }
</style>
