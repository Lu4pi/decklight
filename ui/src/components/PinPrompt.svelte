<script lang="ts">
  // PIN-Abfrage für Zugriffe aus dem Netz.
  let pin = $state("");
  function go(e: SubmitEvent) {
    e.preventDefault();
    const u = new URL(location.href);
    u.searchParams.set("pin", pin.trim());
    location.href = u.toString();
  }
</script>

<form onsubmit={go}>
  <h2>DeckLight</h2>
  <p>Für diese Ansicht ist eine PIN eingestellt.</p>
  <!-- svelte-ignore a11y_autofocus -->
  <input autofocus inputmode="numeric" bind:value={pin} placeholder="PIN" />
  <button class="primary" type="submit">Öffnen</button>
</form>

<style>
  form {
    max-width: 320px;
    margin: 18vh auto;
    display: flex;
    flex-direction: column;
    gap: 12px;
    text-align: center;
  }
  h2 {
    margin: 0;
    color: var(--accent);
  }
  p {
    color: var(--text-dim);
  }
  input {
    font-size: 24px;
    text-align: center;
  }
</style>
