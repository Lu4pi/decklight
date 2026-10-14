<script lang="ts">
  // Einstieg: /monitor und /overlay sind eigene Vollbild-Ansichten, sonst die App.
  import { conn } from "./lib/conn.svelte";
  import Main from "./pages/Main.svelte";
  import Monitor from "./pages/Monitor.svelte";
  import Overlay from "./pages/Overlay.svelte";
  import { hideSplash, splashStatus, splashTimeout } from "./lib/splash";

  const path = location.pathname.replace(/\/+$/, "");
  const view = path.endsWith("/overlay") ? "overlay" : path.endsWith("/monitor") ? "monitor" : "main";

  $effect(() => {
    conn.start();
  });

  // Startbildschirm nur für die Haupt-App; Monitor und Overlay sofort zeigen.
  if (view === "main") splashTimeout();
  else hideSplash(true);
  $effect(() => {
    if (view !== "main") return;
    if (conn.state) {
      splashStatus("Bereit");
      hideSplash();
    } else if (conn.needsPin) {
      hideSplash();
    } else {
      splashStatus(conn.connected ? "Lade Zustand" : "Kern startet");
    }
  });
</script>

{#if view === "overlay"}
  <Overlay />
{:else if view === "monitor"}
  <Monitor />
{:else}
  <Main />
{/if}
