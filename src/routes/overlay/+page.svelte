<script lang="ts">
  // Contador sobre el juego: solo texto, fondo transparente, tres datos.
  import { invoke, listen } from "$lib/tauri";
  import { onMount } from "svelte";
  import { texts, type Lang } from "$lib/i18n";

  type Stats = {
    matchCounts: { total: number };
    totalCounts: { total: number };
    totalMatches: number;
  };
  type Settings = { language: string; corner: string };

  let stats = $state<Stats | null>(null);
  let settings = $state<Settings | null>(null);
  const lang = $derived<Lang>(settings?.language === "en" ? "en" : "es");
  const t = $derived(texts[lang]);
  const right = $derived(settings?.corner.endsWith("right") ?? false);

  onMount(() => {
    invoke<Stats>("get_stats").then((value) => (stats = value));
    invoke<Settings>("get_settings").then((value) => (settings = value));
    const listeners = [
      listen<Stats>("stats-changed", (event) => (stats = event.payload)),
      listen<Settings>("settings-changed", (event) => (settings = event.payload)),
    ];
    return () => {
      for (const listener of listeners) listener.then((stop) => stop());
    };
  });
</script>

{#if stats}
  <div class:right>
    <p>{t.overlayMatch} <b>{stats.totalMatches}</b></p>
    <p>{t.overlayTotal} <b>{stats.totalCounts.total}</b></p>
    <p>{t.overlayThis} <b>{stats.matchCounts.total}</b></p>
  </div>
{/if}

<style>
  :global(html, body) {
    margin: 0;
    background: transparent;
    overflow: hidden;
    user-select: none;
  }

  div {
    padding: 1px 4px;
    font-family: system-ui, "Segoe UI", sans-serif;
    font-size: 15px;
    line-height: 1.45;
    color: #fff;
    /* Contorno oscuro: se lee sobre cualquier fondo sin necesitar recuadro. */
    text-shadow:
      0 0 3px #000,
      0 0 3px #000,
      1px 1px 2px #000;
  }

  div.right {
    text-align: right;
  }

  p {
    margin: 0;
    font-variant-numeric: tabular-nums;
  }

  b {
    font-size: 17px;
  }
</style>
