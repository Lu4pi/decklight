<script lang="ts">
  import { conn } from "../lib/conn.svelte";
  import { time } from "../lib/format";
  import DeckTile from "../components/DeckTile.svelte";

  const decks = $derived((conn.state?.decks ?? []).filter((d) => d.number <= 6));
  const recent = $derived((conn.monitor?.log ?? []).slice(-6).reverse());
</script>

{#if decks.length === 0}
  <div class="empty">
    <h2>Keine Player gefunden</h2>
    <p>
      Prüfe unter <a href="#outputs">Ausgänge</a> die Netzwerkschnittstelle. Zum Ausprobieren ohne Hardware:
      <code>decklight simulate</code>.
    </p>
  </div>
{:else}
  <div class="grid" class:four={decks.length > 2}>
    {#each decks as deck (deck.number)}
      <DeckTile {deck} compact={decks.length > 4} />
    {/each}
  </div>
{/if}

<section class="recent">
  <h3>Zuletzt ausgelöst</h3>
  {#if recent.length === 0}
    <p class="dim">Noch keine Cues oder Trigger ausgelöst.</p>
  {:else}
    <ul>
      {#each recent as e}
        <li class:err={e.error}>
          <span class="t">{time(e.unix_ms)}</span>
          <span class="src">{e.source}{e.deck ? ` · Deck ${e.deck}` : ""}</span>
          <span class="act">{e.actions.join(", ")}</span>
          {#if e.error}<span class="e">{e.error}</span>{/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(420px, 1fr));
    gap: 12px;
  }
  .grid.four {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
  @media (max-width: 900px) {
    .grid.four {
      grid-template-columns: 1fr;
    }
  }
  .empty {
    color: var(--text-dim);
    padding: 40px;
    text-align: center;
  }
  .empty a {
    color: var(--accent);
  }
  .recent {
    margin-top: 16px;
  }
  h3 {
    font-size: 13px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-dim);
    margin: 0 0 6px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    font-size: 13px;
  }
  li {
    display: flex;
    gap: 12px;
    padding: 4px 0;
    border-bottom: 1px solid var(--border);
  }
  .t {
    font-family: var(--mono);
    color: var(--text-dim);
  }
  .src {
    font-weight: 600;
    min-width: 160px;
  }
  .act {
    color: var(--text-dim);
  }
  .e,
  li.err .src {
    color: var(--danger);
  }
  .dim {
    color: var(--text-dim);
    font-size: 13px;
  }
</style>
