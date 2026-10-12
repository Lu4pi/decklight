<script lang="ts">
  // Platzhalter bis zum Webserver (Phase 6): zeigt den Verbindungsstatus zum Kern.
  let connected = $state(false);

  $effect(() => {
    const proto = location.protocol === "https:" ? "wss" : "ws";
    const ws = new WebSocket(`${proto}://${location.host}/ws`);
    ws.onopen = () => (connected = true);
    ws.onclose = () => (connected = false);
    return () => ws.close();
  });
</script>

<main>
  <h1>DeckLight</h1>
  <p class:ok={connected}>{connected ? "Mit dem Kern verbunden" : "Warte auf den DeckLight-Kern …"}</p>
</main>

<style>
  main {
    padding: 2rem;
  }
  h1 {
    margin: 0 0 0.5rem;
    font-weight: 700;
    letter-spacing: 0.02em;
  }
  p {
    color: var(--text-dim);
  }
  p.ok {
    color: var(--ok);
  }
</style>
