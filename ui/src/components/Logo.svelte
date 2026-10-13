<script lang="ts">
  // DeckLight-Wortmarke mit Lichtsymbol.
  let { size = 18 }: { size?: number } = $props();
</script>

<span class="logo" style:font-size="{size}px">
  <svg viewBox="0 0 20 20" aria-hidden="true"><path class="cone" d="M7.5 5 2 19h16L12.5 5z" /><rect x="6.5" y="1.5" width="7" height="3.5" rx="0.8" /></svg>DECK<b>LIGHT</b>
</span>

<style>
  .logo {
    display: inline-flex;
    align-items: center;
    font-weight: 800;
    letter-spacing: 0.12em;
    color: #fff;
    white-space: nowrap;
  }
  b {
    color: var(--beat);
    font-weight: 800;
  }
  svg {
    width: 0.95em;
    height: 0.95em;
    margin: 0 0.35em 0.05em 0;
    fill: var(--beat);
  }
  .cone {
    opacity: 0.4;
  }
</style>
