<script lang="ts">
  // Texto del contador sobre el juego. Lo usan la ventana del contador y la
  // vista previa de Configuración, para que se vean exactamente igual.
  import type { texts } from "$lib/i18n";
  import { clock } from "$lib/format";

  type Counts = { normal: number; rare: number; spirit: number; total: number };
  type Props = {
    stats: {
      matchCounts: Counts;
      totalCounts: Counts;
      dayCounts?: Counts;
      dayMatches: number;
      totalMatches: number;
      paused?: boolean;
      inMatch?: boolean;
      matchStartedAt?: number | null;
    };
    options: {
      overlayMatch: string;
      overlayTotal: boolean;
      overlayChests: string;
      overlayColor: boolean;
      overlaySize: string;
      overlayOpacity: number;
      dailyGoal?: number;
      overlayGoal?: boolean;
      overlayTimer?: boolean;
    };
    t: (typeof texts)["es"];
    right?: boolean;
    /** Si viene, se muestra el resumen de la partida que acaba de terminar. */
    summary?: { durationMs: number; counts: Counts } | null;
    /** Alto de la zona del juego en píxeles lógicos (para el tamaño automático). */
    areaHeight?: number;
    /** Cofre recién contado, para el destello "+1". */
    flash?: { kind: "normal" | "rare" | "spirit"; id: number } | null;
    /** Hora actual (epoch ms), para el tiempo de partida. */
    now?: number;
    /** Problema para ver el juego: se avisa encima de todo. */
    problem?: string;
  };
  let {
    stats,
    options,
    t,
    right = false,
    summary = null,
    areaHeight = 1080,
    flash = null,
    now = Date.now(),
    problem = "none",
  }: Props = $props();
  /** "12:34" (o "1:02:03" si pasa de una hora). */
  function elapsed(since: number) {
    const s = Math.max(0, Math.floor((now - since) / 1000));
    const pad = (n: number) => String(n).padStart(2, "0");
    const h = Math.floor(s / 3600);
    const m = Math.floor(s / 60) % 60;
    return h > 0 ? `${h}:${pad(m)}:${pad(s % 60)}` : `${m}:${pad(s % 60)}`;
  }
  const sizes: Record<string, number> = { small: 13, medium: 15, large: 19 };
  // Automático: 15 px a 1080 de alto, proporcional, entre 12 y 24.
  const fontSize = $derived(
    options.overlaySize === "auto"
      ? Math.min(24, Math.max(12, Math.round((15 * areaHeight) / 1080)))
      : (sizes[options.overlaySize] ?? 15),
  );
  const flashClass = { normal: "tesoro", rare: "raro", spirit: "espiritu" } as const;
</script>

<div
  class:right
  class:color={options.overlayColor}
  style:font-size="{fontSize}px"
  style:opacity={options.overlayOpacity / 100}
>
  {#if problem !== "none" && t.problemsShort[problem]}<p class="problema">⚠ {t.problemsShort[problem]}</p>{/if}
  {#if stats.paused}<p class="pausa">⏸ {t.overlayPaused}</p>{/if}
  {#if flash}
    {#key flash.id}
      <p class="destello {flashClass[flash.kind]}">+1 {({ normal: t.treasure, rare: t.rare, spirit: t.spirit })[flash.kind]}</p>
    {/key}
  {/if}
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
      <b>{options.overlayMatch === "day" ? stats.dayMatches : stats.totalMatches}</b>
    </p>
  {/if}
  {#if options.overlayTotal}
    <p>{t.overlayTotal} <b>{stats.totalCounts.total}</b></p>
  {/if}
  {#if options.overlayTimer && stats.inMatch && stats.matchStartedAt}
    <p>{t.overlayTime} <b>{elapsed(stats.matchStartedAt)}</b></p>
  {/if}
  {#if options.overlayGoal && options.dailyGoal && stats.dayCounts}
    {@const done = stats.dayCounts.total}
    <p class:meta={done >= options.dailyGoal}>
      {t.overlayGoal} <b>{done}/{options.dailyGoal}</b>{#if done >= options.dailyGoal} ✓{/if}
    </p>
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

  /* El aviso de que no se ve el juego: siempre en ámbar, se use o no color. */
  .problema {
    color: #fbbf24;
    font-weight: 700;
  }

  .color .meta {
    color: #86efac;
  }

  /* "+1" al contar un cofre: aparece con un pequeño salto y se desvanece. */
  .destello {
    font-weight: 700;
    animation: destello 1.6s ease-out forwards;
  }

  .destello.tesoro {
    color: #fbbf24;
  }

  .destello.raro {
    color: #7dd3fc;
  }

  .destello.espiritu {
    color: #f0abfc;
  }

  @keyframes destello {
    0% {
      opacity: 0;
      transform: translateY(4px) scale(0.9);
    }
    15% {
      opacity: 1;
      transform: none;
    }
    75% {
      opacity: 1;
    }
    100% {
      opacity: 0;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .destello {
      animation: none;
    }
  }
</style>
