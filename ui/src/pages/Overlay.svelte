<script lang="ts">
  // /overlay: transparentes Overlay für OBS und Streams.
  // URL-Parameter: theme=dark|light|glass, layout=card|bar, deck=master|1..6,
  // accent=ff8a1f (Hex ohne #), hide=cover,bpm,progress,artist
  import { conn } from "../lib/conn.svelte";
  import { artworkUrl } from "../lib/tracks";

  const q = new URLSearchParams(location.search);
  const theme = q.get("theme") ?? "dark";
  const layout = q.get("layout") ?? "card";
  const deckParam = q.get("deck") ?? "master";
  const accent = /^[0-9a-f]{6}$/i.test(q.get("accent") ?? "") ? `#${q.get("accent")}` : "#ffb020";
  const hide = new Set((q.get("hide") ?? "").split(",").map((s) => s.trim()));

  $effect(() => {
    document.documentElement.style.background = "transparent";
    document.body.style.background = "transparent";
  });

  const deck = $derived.by(() => {
    const decks = conn.state?.decks ?? [];
    if (deckParam === "master") return decks.find((d) => d.master) ?? null;
    return decks.find((d) => d.number === Number(deckParam)) ?? null;
  });

  let progress = $state(0);
  $effect(() => {
    let raf = 0;
    const loop = () => {
      const d = deck;
      const len = d?.track_length_ms ?? (d?.info ? d.info.duration_s * 1000 : 0);
      const pos = d ? conn.positionOf(d.number) : null;
      progress = len && pos != null ? Math.min(1, pos / len) : 0;
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  });

  const visible = $derived(!!deck?.info);
</script>

<div class="overlay {theme} layout-{layout}" class:visible style:--accent={accent}>
  {#if deck?.info}
    {#if !hide.has("cover") && deck.info.has_artwork}
      {#key deck.info.key}<img src={artworkUrl(deck.info.key)} alt="" />{/key}
    {/if}
    <div class="text">
      <div class="title">{deck.info.title}</div>
      {#if !hide.has("artist")}<div class="artist">{deck.info.artist}</div>{/if}
      {#if !hide.has("progress")}
        <div class="bar"><div class="fill" style:width="{progress * 100}%"></div></div>
      {/if}
    </div>
    {#if !hide.has("bpm") && deck.effective_bpm}
      <div class="bpm">{deck.effective_bpm.toFixed(0)}<small>BPM</small></div>
    {/if}
  {/if}
</div>

<style>
  .overlay {
    position: fixed;
    left: 24px;
    bottom: 24px;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 12px 16px;
    border-radius: 14px;
    min-width: 320px;
    max-width: min(720px, calc(100vw - 48px));
    opacity: 0;
    transform: translateY(12px);
    transition:
      opacity 0.4s,
      transform 0.4s;
    font-family: var(--font);
  }
  .overlay.visible {
    opacity: 1;
    transform: none;
  }
  .dark {
    background: rgba(11, 13, 16, 0.86);
    color: #fff;
  }
  .light {
    background: rgba(255, 255, 255, 0.92);
    color: #111;
  }
  .glass {
    background: rgba(255, 255, 255, 0.12);
    backdrop-filter: blur(12px);
    color: #fff;
    border: 1px solid rgba(255, 255, 255, 0.25);
  }
  .overlay.layout-bar {
    left: 0;
    right: 0;
    bottom: 0;
    border-radius: 0;
    max-width: none;
  }
  img {
    width: 64px;
    height: 64px;
    border-radius: 8px;
    object-fit: cover;
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  .title {
    font-size: 22px;
    font-weight: 800;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .artist {
    font-size: 16px;
    opacity: 0.75;
  }
  .bar {
    height: 4px;
    background: rgba(127, 127, 127, 0.3);
    border-radius: 2px;
    margin-top: 8px;
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--accent);
  }
  .bpm {
    font-family: var(--mono);
    font-size: 28px;
    font-weight: 800;
    color: var(--accent);
  }
  .bpm small {
    font-size: 11px;
    margin-left: 3px;
    opacity: 0.8;
  }
</style>
