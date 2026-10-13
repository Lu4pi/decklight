<script lang="ts">
  // Sieben-Segment-Text mit unbeleuchteten „8“ dahinter, damit schmale Ziffern
  // (die „1“) nicht wie Lücken aussehen. Schrift und Farbe erbt er vom Elternelement.
  let { text }: { text: string } = $props();
  const ghost = $derived(text.replace(/[0-9 -]/g, "8"));
</script>

<span class="st"><span class="g" aria-hidden="true">{ghost}</span><span class="v">{text.replace(/ /g, "!")}</span></span>

<style>
  .st {
    position: relative;
    display: inline-block;
    font-family: var(--seg);
    white-space: pre;
  }
  .g {
    opacity: 0.1;
    color: #fff;
  }
  .v {
    position: absolute;
    inset: 0;
  }
</style>
