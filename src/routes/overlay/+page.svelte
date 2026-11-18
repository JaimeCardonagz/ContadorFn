<script lang="ts">
  // Contador sobre el juego: solo texto, fondo transparente.
  import { invoke, listen, startDragging } from "$lib/tauri";
  import { onMount } from "svelte";
  import { texts, type Lang } from "$lib/i18n";
  import OverlayText from "$lib/OverlayText.svelte";

  type Counts = { normal: number; rare: number; spirit: number; total: number };
  type Stats = {
    matchCounts: Counts;
    totalCounts: Counts;
    sessionCounts: Counts;
    sessionMatches: number;
    totalMatches: number;
    paused?: boolean;
  };
  type Settings = {
    language: string;
    corner: string;
    overlayMatch: string;
    overlayTotal: boolean;
    overlayChests: string;
    overlayColor: boolean;
    overlaySize: string;
    overlayOpacity: number;
    matchSummary: boolean;
    overlayX: number;
    overlayY: number;
    overlayFlash: boolean;
    dailyGoal: number;
    overlayGoal: boolean;
  };

  let stats = $state<Stats | null>(null);
  let settings = $state<Settings | null>(null);
  const lang = $derived<Lang>(settings?.language === "en" ? "en" : "es");

  // Resumen de la partida que acaba de terminar, unos segundos.
  const SUMMARY_MS = 8000;
  type Summary = { durationMs: number; counts: Counts };
  let summary = $state<Summary | null>(null);
  let summaryTimer: ReturnType<typeof setTimeout> | undefined;
  function showSummary(value: Summary) {
    if (!settings?.matchSummary) return;
    summary = value;
    clearTimeout(summaryTimer);
    summaryTimer = setTimeout(() => (summary = null), SUMMARY_MS);
  }

  // Colocándolo: se arrastra con el ratón; Intro guarda y Esc cancela.
  let moving = $state(false);
  const finish = (save: boolean) => invoke("finish_overlay_move", { save }).catch(() => {});
  function onKey(event: KeyboardEvent) {
    if (!moving) return;
    if (event.key === "Enter") finish(true);
    if (event.key === "Escape") finish(false);
  }

  // Alineación del texto: según la esquina o, en posición personalizada,
  // según en qué mitad de la pantalla quedó.
  const custom = $derived(settings?.corner === "custom");
  const right = $derived(!!settings && (settings.corner.endsWith("right") || (custom && settings.overlayX > 500)));
  const bottom = $derived(!!settings && (settings.corner.startsWith("bottom") || (custom && settings.overlayY > 500)));

  // Alto de la zona del juego (tamaño automático del texto).
  let areaHeight = $state(1080);

  // Destello "+1": cuando sube el total de un tipo de cofre.
  type Kind = "normal" | "rare" | "spirit";
  let flash = $state<{ kind: Kind; id: number } | null>(null);
  let flashTimer: ReturnType<typeof setTimeout> | undefined;
  let previous: Counts | null = null;
  function onStats(next: Stats) {
    const before = previous;
    previous = next.totalCounts;
    stats = next;
    if (!before || !settings?.overlayFlash) return;
    const kind = (["normal", "rare", "spirit"] as const).find((k) => next.totalCounts[k] > before[k]);
    if (!kind) return;
    flash = { kind, id: Date.now() };
    clearTimeout(flashTimer);
    flashTimer = setTimeout(() => (flash = null), 1700);
  }

  // La ventana del contador se ajusta a lo que muestra.
  let content = $state<HTMLElement | null>(null);
  $effect(() => {
    if (!content) return;
    const node = content;
    let last = "";
    const fit = () => {
      const width = Math.ceil(node.scrollWidth) + 2;
      const height = Math.ceil(node.scrollHeight) + 2;
      const key = `${width}x${height}`;
      if (key === last) return;
      last = key;
      invoke("fit_overlay", { width, height }).catch(() => {});
    };
    const observer = new ResizeObserver(fit);
    observer.observe(node);
    fit();
    return () => observer.disconnect();
  });

  onMount(() => {
    invoke<Stats>("get_stats").then(onStats);
    invoke<Settings>("get_settings").then((value) => (settings = value));
    const listeners = [
      listen<Stats>("stats-changed", (event) => onStats(event.payload)),
      listen<number>("overlay-area", (event) => (areaHeight = event.payload)),
      listen<Settings>("settings-changed", (event) => (settings = event.payload)),
      listen<Summary>("match-summary", (event) => showSummary(event.payload)),
      listen<boolean>("overlay-moving", (event) => (moving = event.payload)),
    ];
    return () => {
      for (const listener of listeners) listener.then((stop) => stop());
      clearTimeout(summaryTimer);
      clearTimeout(flashTimer);
    };
  });
</script>

<svelte:window onkeydown={onKey} />

{#if stats && settings}
  <!-- En las esquinas de abajo el texto se pega al borde inferior. -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <main
    class:abajo={bottom && !moving}
    class:moviendo={moving}
    onmousedown={(event) => moving && event.button === 0 && startDragging()}
  >
    <div class="contenido" bind:this={content}>
      {#if moving}<span class="pista">{texts[lang].moveDrag}</span>{/if}
      <OverlayText {stats} {summary} {areaHeight} {flash} options={settings} t={texts[lang]} right={right && !moving} />
    </div>
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

  /* Mide lo que ocupa el texto, sin depender del tamaño de la ventana. */
  .contenido {
    width: max-content;
  }

  /* Colocándolo: marco discontinuo, fondo oscuro y cursor de mover. */
  main.moviendo {
    box-sizing: border-box;
    border: 2px dashed #fbbf24;
    border-radius: 8px;
    background: rgba(15, 15, 35, 0.72);
    cursor: move;
  }

  .pista {
    padding: 2px 6px;
    font: 600 11px system-ui, sans-serif;
    color: #fbbf24;
  }
</style>
