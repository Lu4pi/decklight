<script lang="ts">
  import { toasts } from "../lib/toast.svelte";
</script>

<div class="toasts">
  {#each toasts.list as t (t.id)}
    <div class="toast" class:error={t.kind === "error"}>{t.text}</div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    right: 16px;
    bottom: 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: 100;
  }
  .toast {
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-left: 3px solid var(--ok);
    padding: 8px 14px;
    border-radius: 6px;
    font-size: 14px;
    max-width: 420px;
  }
  .toast.error {
    border-left-color: var(--danger);
  }
</style>
