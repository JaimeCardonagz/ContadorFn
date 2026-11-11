<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  type Counts = { normal: number; rare: number; spirit: number; boss: number; total: number };
  type Stats = {
    inMatch: boolean;
    sessionActive: boolean;
    sessionStartedAt: number | null;
    matchCounts: Counts;
    sessionCounts: Counts;
    totalCounts: Counts;
  };

  let stats = $state<Stats | null>(null);
  let debug = $state(false);
  let error = $state("");
  let now = $state(Date.now());

  async function run(command: string, args?: Record<string, unknown>) {
    try {
      stats = await invoke<Stats>(command, args);
      error = "";
    } catch (e) {
      error = String(e);
    }
  }

  onMount(() => {
    run("get_stats");
    invoke<boolean>("is_debug").then((value) => (debug = value));
    const unlisten = listen<Stats>("stats-changed", (event) => (stats = event.payload));
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => {
      unlisten.then((stop) => stop());
      clearInterval(timer);
    };
  });

  function breakdown(c: Counts) {
    return `Normal ${c.normal} · Raro ${c.rare} · Espíritu ${c.spirit} · Jefe ${c.boss}`;
  }

  function elapsed(since: number | null) {
    if (since === null) return "";
    const s = Math.max(0, Math.floor((now - since) / 1000));
    const pad = (n: number) => String(n).padStart(2, "0");
    return `${pad(Math.floor(s / 3600))}:${pad(Math.floor(s / 60) % 60)}:${pad(s % 60)}`;
  }

  // Ajuste manual del histórico: cofres abiertos antes de usar la app, o correcciones.
  let adjustType = $state("");
  let adjustAmount = $state<number | null>(null);

  async function adjust() {
    if (!adjustAmount || !Number.isInteger(adjustAmount)) return;
    await run("adjust_total", { chest: adjustType || null, delta: adjustAmount });
    if (!error) adjustAmount = null;
  }

  const simulated = [
    ["match_start", "Inicia partida"],
    ["normal", "+ Normal"],
    ["rare", "+ Raro"],
    ["spirit", "+ Espíritu"],
    ["boss", "+ Jefe"],
    ["eliminated", "Eliminado"],
    ["victory", "Victoria"],
    ["lobby", "Lobby"],
  ];
</script>

<main>
  {#if stats}
    <header class:activo={stats.inMatch}>
      ● {stats.inMatch ? "EN PARTIDA" : stats.sessionActive ? "SESIÓN ACTIVA" : "SIN SESIÓN"}
    </header>

    <section>
      <div class="fila"><span>PARTIDA</span><strong>{stats.matchCounts.total}</strong></div>
      <small>{breakdown(stats.matchCounts)}</small>
    </section>

    <section>
      <div class="fila">
        <span>HOY <small>{elapsed(stats.sessionStartedAt)}</small></span>
        <strong>{stats.sessionCounts.total}</strong>
      </div>
      <small>{breakdown(stats.sessionCounts)}</small>
    </section>

    <section>
      <div class="fila"><span>HISTÓRICO</span><strong>{stats.totalCounts.total}</strong></div>
    </section>

    <nav>
      <button onclick={() => run("start_session")} disabled={stats.sessionActive}>Iniciar</button>
      <button onclick={() => run("stop_session")} disabled={!stats.sessionActive}>Detener</button>
      <button onclick={() => run("new_match")}>Nueva partida</button>
    </nav>

    <form onsubmit={(e) => (e.preventDefault(), adjust())}>
      <label for="ajuste">Ajustar histórico</label>
      <select bind:value={adjustType}>
        <option value="">Sin tipo</option>
        <option value="normal">Normal</option>
        <option value="rare">Raro</option>
        <option value="spirit">Espíritu</option>
        <option value="boss">Jefe</option>
      </select>
      <input id="ajuste" type="number" step="1" placeholder="±cantidad" bind:value={adjustAmount} />
      <button type="submit" disabled={!adjustAmount}>Aplicar</button>
    </form>

    {#if debug}
      <nav class="debug">
        {#each simulated as [event, label]}
          <button onclick={() => run("debug_simulate", { event })}>{label}</button>
        {/each}
      </nav>
    {/if}
  {/if}

  {#if error}<p class="error">{error}</p>{/if}
</main>

<style>
  :global(html, body) {
    margin: 0;
    background: #14161c;
    color: #e8eaf0;
    font-family: system-ui, "Segoe UI", sans-serif;
    font-size: 14px;
    user-select: none;
  }

  main {
    display: flex;
    flex-direction: column;
    padding: 10px 14px;
    gap: 8px;
  }

  header {
    font-size: 12px;
    letter-spacing: 0.06em;
    color: #9aa1b2;
  }

  header.activo {
    color: #5fd38d;
  }

  section {
    border-top: 1px solid #2a2e3a;
    padding-top: 8px;
  }

  .fila {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    letter-spacing: 0.06em;
  }

  strong {
    font-size: 22px;
    font-variant-numeric: tabular-nums;
  }

  small {
    color: #9aa1b2;
    font-variant-numeric: tabular-nums;
  }

  nav {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    border-top: 1px solid #2a2e3a;
    padding-top: 8px;
  }

  form {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    border-top: 1px solid #2a2e3a;
    padding-top: 8px;
  }

  form label {
    flex-basis: 100%;
    font-size: 12px;
    color: #9aa1b2;
  }

  input {
    width: 90px;
  }

  button,
  select,
  input {
    background: #262a36;
    color: inherit;
    border: 1px solid #3a4050;
    border-radius: 4px;
    padding: 4px 8px;
    font: inherit;
    cursor: pointer;
  }

  button:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .debug button {
    font-size: 12px;
    border-style: dashed;
  }

  .error {
    color: #ff7b7b;
    margin: 0;
  }
</style>
