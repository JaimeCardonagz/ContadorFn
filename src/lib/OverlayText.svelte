<script lang="ts">
  // Texto del contador sobre el juego. Lo usan la ventana del contador y la
  // vista previa de Configuración, para que se vean exactamente igual.
  import type { texts } from "$lib/i18n";
  import { clock } from "$lib/format";

  type Counts = { normal: number; rare: number; spirit: number; total: number };
  type Props = {
    stats: { matchCounts: Counts; totalCounts: Counts; sessionMatches: number; totalMatches: number; paused?: boolean };
    options: {
      overlayMatch: string;
      overlayTotal: boolean;
      overlayChests: string;
      overlayColor: boolean;
      overlaySize: string;
      overlayOpacity: number;
    };
    t: (typeof texts)["es"];
    right?: boolean;
    /** Si viene, se muestra el resumen de la partida que acaba de terminar. */
    summary?: { durationMs: number; counts: Counts } | null;
  };
  let { stats, options, t, right = false, summary = null }: Props = $props();
  const sizes: Record<string, number> = { small: 13, medium: 15, large: 19 };
</script>

<div
  class:right
  class:color={options.overlayColor}
  style:font-size="{sizes[options.overlaySize] ?? 15}px"
  style:opacity={options.overlayOpacity / 100}
>
  {#if stats.paused}<p class="pausa">⏸ {t.overlayPaused}</p>{/if}
  {#if summary}
    <p>{t.summaryTitle}</p>
    <p><b>{summary.counts.total}</b> {t.summaryChests(summary.counts.total)} · {clock(summary.durationMs)}</p>
    <p>
      <span class="tesoro">{t.treasure[0]} <b>{summary.counts.normal}</b></span>
      <span class="raro">{t.rare[0]} <b>{summary.counts.rare}</b></span>
      <span class="espiritu">{t.spirit[0]} <b>{summary.counts.spirit}</b></span>
    </p>
  {:else}
  {#if options.overlayMatch !== "off"}
    <p>
      {t.overlayMatch}
      <b>{options.overlayMatch === "day" ? stats.sessionMatches : stats.totalMatches}</b>
    </p>
  {/if}
  {#if options.overlayTotal}
    <p>{t.overlayTotal} <b>{stats.totalCounts.total}</b></p>
  {/if}
  {#if options.overlayChests === "sum"}
    <p>{t.overlayThis} <b>{stats.matchCounts.total}</b></p>
  {:else if options.overlayChests === "types"}
    <p>
      <span class="tesoro">{t.treasure[0]} <b>{stats.matchCounts.normal}</b></span>
      <span class="raro">{t.rare[0]} <b>{stats.matchCounts.rare}</b></span>
      <span class="espiritu">{t.spirit[0]} <b>{stats.matchCounts.spirit}</b></span>
    </p>
  {/if}
  {/if}
</div>

<style>
  div {
    padding: 1px 4px;
    font-family: "Segoe UI", system-ui, sans-serif;
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
    white-space: nowrap;
  }

  b {
    font-size: 1.15em;
  }

  span + span {
    margin-left: 8px;
  }

  .color .tesoro {
    color: #fbbf24;
  }

  .color .raro {
    color: #7dd3fc;
  }

  .color .espiritu {
    color: #f0abfc;
  }
  .pausa {
    color: #fbbf24;
  }
</style>
