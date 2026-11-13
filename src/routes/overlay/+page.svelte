<script lang="ts">
  // Contador sobre el juego: solo texto, fondo transparente.
  import { invoke, listen } from "$lib/tauri";
  import { onMount } from "svelte";
  import { texts, type Lang } from "$lib/i18n";
  import OverlayText from "$lib/OverlayText.svelte";

  type Counts = { normal: number; rare: number; spirit: number; total: number };
  type Stats = { matchCounts: Counts; totalCounts: Counts; sessionMatches: number; totalMatches: number };
  type Settings = {
    language: string;
    corner: string;
    overlayMatch: string;
    overlayTotal: boolean;
    overlayChests: string;
    overlayColor: boolean;
    overlaySize: string;
    overlayOpacity: number;
  };

  let stats = $state<Stats | null>(null);
  let settings = $state<Settings | null>(null);
  const lang = $derived<Lang>(settings?.language === "en" ? "en" : "es");

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

{#if stats && settings}
  <!-- En las esquinas de abajo el texto se pega al borde inferior. -->
  <main class:abajo={settings.corner.startsWith("bottom")}>
    <OverlayText {stats} options={settings} t={texts[lang]} right={settings.corner.endsWith("right")} />
  </main>
{/if}

<style>
  :global(html, body) {
    margin: 0;
    background: transparent;
    overflow: hidden;
    user-select: none;
  }

  main {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }

  main.abajo {
    justify-content: flex-end;
  }
</style>
