<script lang="ts">
  // Einstieg: /monitor und /overlay sind eigene Vollbild-Ansichten, sonst die App.
  import { conn } from "./lib/conn.svelte";
  import Main from "./pages/Main.svelte";
  import Monitor from "./pages/Monitor.svelte";
  import Overlay from "./pages/Overlay.svelte";

  const path = location.pathname.replace(/\/+$/, "");
  const view = path.endsWith("/overlay") ? "overlay" : path.endsWith("/monitor") ? "monitor" : "main";

  $effect(() => {
    conn.start();
  });
</script>

{#if view === "overlay"}
  <Overlay />
{:else if view === "monitor"}
  <Monitor />
{:else}
  <Main />
{/if}
