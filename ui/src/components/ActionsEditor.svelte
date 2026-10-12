<script lang="ts">
  // Liste von Aktionen bearbeiten: grandMA3-Befehl, OSC-Adresse, MIDI.
  import type { Action, MidiMessage, OscArg } from "../lib/types";

  let { actions = $bindable(), disabled = false }: { actions: Action[]; disabled?: boolean } = $props();

  type Kind = "osc" | "osc_message" | "note_on" | "note_off" | "control_change" | "program_change";

  function kindOf(a: Action): Kind {
    return a.type === "midi" ? a.message.type : a.type;
  }

  function make(kind: Kind): Action {
    switch (kind) {
      case "osc":
        return { type: "osc", command: "Go+ Exec 201" };
      case "osc_message":
        return { type: "osc_message", address: "/Page1/Fader201", args: [{ type: "float", value: 100 }] };
      case "note_on":
        return { type: "midi", message: { type: "note_on", channel: 1, note: 60, velocity: 127 } };
      case "note_off":
        return { type: "midi", message: { type: "note_off", channel: 1, note: 60 } };
      case "control_change":
        return { type: "midi", message: { type: "control_change", channel: 1, controller: 1, value: 127 } };
      case "program_change":
        return { type: "midi", message: { type: "program_change", channel: 1, program: 0 } };
    }
  }

  function setKind(i: number, kind: Kind) {
    actions[i] = make(kind);
  }

  function midi(a: Action): MidiMessage | null {
    return a.type === "midi" ? a.message : null;
  }

  function num(v: string, min: number, max: number): number {
    const n = Math.round(Number(v));
    return Math.max(min, Math.min(max, isFinite(n) ? n : min));
  }

  function argsText(a: Action): string {
    if (a.type !== "osc_message") return "";
    return a.args.map((x) => (x.type === "string" ? JSON.stringify(x.value) : String(x.value))).join(" ");
  }

  function parseArgs(text: string): OscArg[] {
    const out: OscArg[] = [];
    const re = /"((?:[^"\\]|\\.)*)"|(\S+)/g;
    let m: RegExpExecArray | null;
    while ((m = re.exec(text))) {
      if (m[1] !== undefined) out.push({ type: "string", value: m[1].replace(/\\"/g, '"') });
      else if (/^-?\d+$/.test(m[2])) out.push({ type: "int", value: Number(m[2]) });
      else if (/^-?\d*\.\d+$/.test(m[2])) out.push({ type: "float", value: Number(m[2]) });
      else out.push({ type: "string", value: m[2] });
    }
    return out;
  }
</script>

<div class="actions">
  {#each actions as a, i}
    <div class="row">
      <select value={kindOf(a)} {disabled} onchange={(e) => setKind(i, (e.target as HTMLSelectElement).value as Kind)}>
        <option value="osc">MA3-Befehl (OSC)</option>
        <option value="osc_message">OSC-Adresse</option>
        <option value="note_on">MIDI Note an</option>
        <option value="note_off">MIDI Note aus</option>
        <option value="control_change">MIDI CC</option>
        <option value="program_change">MIDI Program</option>
      </select>

      {#if a.type === "osc"}
        <input class="wide" {disabled} bind:value={a.command} placeholder="Go+ Exec 201 · {'{deck}'} = Decknummer" />
      {:else if a.type === "osc_message"}
        <input {disabled} bind:value={a.address} placeholder="/Adresse" />
        <input
          class="wide"
          {disabled}
          value={argsText(a)}
          placeholder={'Argumente: 1 0.5 "Text"'}
          onchange={(e) => {
            if (a.type === "osc_message") a.args = parseArgs((e.target as HTMLInputElement).value);
          }}
        />
      {:else}
        {@const m = midi(a)}
        {#if m}
          <label>Kanal <input type="number" min="1" max="16" {disabled} value={m.channel}
              onchange={(e) => (m.channel = num((e.target as HTMLInputElement).value, 1, 16))} /></label>
          {#if m.type === "note_on" || m.type === "note_off"}
            <label>Note <input type="number" min="0" max="127" {disabled} value={m.note}
                onchange={(e) => (m.note = num((e.target as HTMLInputElement).value, 0, 127))} /></label>
          {/if}
          {#if m.type === "note_on"}
            <label>Vel <input type="number" min="0" max="127" {disabled} value={m.velocity}
                onchange={(e) => (m.velocity = num((e.target as HTMLInputElement).value, 0, 127))} /></label>
          {/if}
          {#if m.type === "control_change"}
            <label>CC <input type="number" min="0" max="127" {disabled} value={m.controller}
                onchange={(e) => (m.controller = num((e.target as HTMLInputElement).value, 0, 127))} /></label>
            <label>Wert <input type="number" min="0" max="127" {disabled} value={m.value}
                onchange={(e) => (m.value = num((e.target as HTMLInputElement).value, 0, 127))} /></label>
          {/if}
          {#if m.type === "program_change"}
            <label>Program <input type="number" min="0" max="127" {disabled} value={m.program}
                onchange={(e) => (m.program = num((e.target as HTMLInputElement).value, 0, 127))} /></label>
          {/if}
        {/if}
      {/if}
      {#if !disabled}
        <button class="icon" title="Aktion entfernen" onclick={() => actions.splice(i, 1)}>✕</button>
      {/if}
    </div>
  {/each}
  {#if !disabled}
    <button class="link" onclick={() => actions.push(make("osc"))}>+ Aktion</button>
  {/if}
</div>

<style>
  .actions {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .row {
    display: flex;
    gap: 6px;
    align-items: center;
    flex-wrap: wrap;
  }
  .wide {
    flex: 1;
    min-width: 220px;
  }
  label {
    font-size: 12px;
    color: var(--text-dim);
    display: flex;
    align-items: center;
    gap: 4px;
  }
  input[type="number"] {
    width: 64px;
  }
</style>
