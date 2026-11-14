<script lang="ts">
  import { appVersion, invoke, listen } from "$lib/tauri";
  import { isNewer, latestVersion } from "$lib/updates";
  import { onMount } from "svelte";
  import { texts, type Lang } from "$lib/i18n";
  import OverlayText from "$lib/OverlayText.svelte";
  import Help from "$lib/Help.svelte";

  type Counts = { normal: number; rare: number; spirit: number; total: number };
  type Stats = {
    inMatch: boolean;
    sessionActive: boolean;
    sessionStartedAt: number | null;
    matchCounts: Counts;
    sessionCounts: Counts;
    totalCounts: Counts;
    sessionMatches: number;
    totalMatches: number;
  };

  type Diag = {
    enabled: boolean;
    supported: boolean;
    dir: string;
    inputDown: boolean;
    inputPresses: number;
    pollHits: number;
    hotkeyHits: number;
    hotkeyError: string | null;
    pendingIn: number | null;
    recording: boolean;
    frames: number;
    lastResult: { ok: boolean; message: string } | null;
    detected: number;
    detectedWithInput: number;
    lastDetected: string | null;
    detectError: string | null;
    hudVisible: boolean;
  };

  type Binding =
    | { device: "key"; code: number; label: string }
    | { device: "mouse"; button: number }
    | { device: "pad"; button: number };
  type Settings = {
    language: string;
    binding: Binding;
    requireInput: boolean;
    hideFromCapture: boolean;
    showOverlay: boolean;
    corner: string;
    setupDone: boolean;
    saveSamples: boolean;
    diagnostics: boolean;
    checkUpdates: boolean;
    overlayMatch: string;
    overlayTotal: boolean;
    overlayChests: string;
    overlayColor: boolean;
    overlaySize: string;
    overlayOpacity: number;
  };

  let stats = $state<Stats | null>(null);
  let diag = $state<Diag | null>(null);
  let settings = $state<Settings | null>(null);
  let thumbnail = $state("");
  let debug = $state(false);
  let error = $state("");
  let now = $state(Date.now());
  type Tab = "counter" | "stats" | "history" | "settings" | "diag";
  let tab = $state<Tab>("counter");

  // Historial de las últimas 24 h: solo duración y cofres de cada partida.
  type Summary = { id: number; durationMs: number; counts: Counts };
  let history = $state<Summary[] | null>(null);
  const loadHistory = () =>
    invoke<Summary[]>("get_history")
      .then((value) => (history = value))
      .catch((e) => (error = String(e)));
  $effect(() => {
    if (tab === "history") loadHistory();
  });

  // Sin modo diagnóstico no hay pestaña Diagnóstico.
  $effect(() => {
    if (tab === "diag" && settings && !settings.diagnostics) tab = "counter";
  });

  // Aviso de versión nueva.
  let update = $state<string | null>(null);

  // Idioma de la interfaz: el guardado o, la primera vez, el del sistema.
  const lang = $derived<Lang>(
    settings?.language === "en" || settings?.language === "es"
      ? settings.language
      : navigator.language.toLowerCase().startsWith("es")
        ? "es"
        : "en",
  );
  const t = $derived(texts[lang]);

  async function run(command: string, args?: Record<string, unknown>) {
    try {
      stats = await invoke<Stats>(command, args);
      error = "";
    } catch (e) {
      error = String(e);
    }
  }

  async function save(changes: Partial<Settings>) {
    if (!settings) return;
    settings = { ...settings, ...changes };
    try {
      await invoke("set_settings", { settings });
      error = "";
    } catch (e) {
      error = String(e);
    }
  }

  onMount(() => {
    run("get_stats");
    invoke<boolean>("is_debug").then((value) => (debug = value));
    invoke<Diag>("get_diag").then((value) => (diag = value));
    invoke<Settings>("get_settings").then((value) => {
      settings = value;
      // La primera vez se guarda el idioma del sistema, para que el backend
      // genere sus mensajes en el mismo idioma que la interfaz.
      if (!value.language) save({ language: lang });
      if (value.checkUpdates) {
        Promise.all([latestVersion(), appVersion()]).then(([latest, current]) => {
          if (latest && isNewer(latest, current)) update = latest;
        });
      }
    });
    const listeners = [
      listen<Stats>("stats-changed", (event) => {
        stats = event.payload;
        if (tab === "history") loadHistory();
      }),
      listen<Diag>("diag-state", (event) => (diag = event.payload)),
      listen<string>("diag-thumbnail", (event) => (thumbnail = event.payload)),
    ];
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => {
      for (const listener of listeners) listener.then((stop) => stop());
      clearInterval(timer);
    };
  });

  /** Duración legible: "23 min 05 s", o "1 h 02 min" si pasa de una hora. */
  function clock(ms: number) {
    const s = Math.max(0, Math.round(ms / 1000));
    const pad = (n: number) => String(n).padStart(2, "0");
    const h = Math.floor(s / 3600);
    const m = Math.floor(s / 60) % 60;
    if (h > 0) return `${h} h ${pad(m)} min`;
    return m > 0 ? `${m} min ${pad(s % 60)} s` : `${s} s`;
  }

  function elapsed(since: number | null) {
    if (since === null) return "";
    const s = Math.max(0, Math.floor((now - since) / 1000));
    const pad = (n: number) => String(n).padStart(2, "0");
    return `${pad(Math.floor(s / 3600))}:${pad(Math.floor(s / 60) % 60)}:${pad(s % 60)}`;
  }

  // Primera vez: lo que el jugador ya llevaba antes de usar el programa.
  let initial = $state({ normal: 0, rare: 0, spirit: 0, matches: 0 });

  async function finishSetup() {
    const whole = (n: number) => Math.max(0, Math.trunc(n || 0));
    await run("initial_setup", {
      normal: whole(initial.normal),
      rare: whole(initial.rare),
      spirit: whole(initial.spirit),
      matches: whole(initial.matches),
    });
    if (!error) await save({ setupDone: true });
  }

  // Reiniciar: pide confirmación y vuelve a la pantalla inicial.
  let confirmingReset = $state(false);

  // Botón con el que el jugador abre los contenedores.
  let capturing = $state(false);

  function bindingLabel(binding: Binding) {
    if (binding.device === "key") return `${t.key} ${binding.label}`;
    if (binding.device === "mouse") return t.mouseButtons[binding.button] ?? `${t.mouse} ${binding.button}`;
    return `${t.pad}: ${t.padButtons[binding.button] ?? binding.button}`;
  }

  function startCapture() {
    // Con un pequeño retardo, para no capturar el propio clic en el botón.
    setTimeout(() => (capturing = true), 200);
  }

  function captureKey(event: KeyboardEvent) {
    if (!capturing) return;
    event.preventDefault();
    capturing = false;
    const label = event.key.length === 1 ? event.key.toUpperCase() : event.code || event.key;
    save({ binding: { device: "key", code: event.keyCode, label } });
  }

  function captureMouse(event: MouseEvent) {
    if (!capturing) return;
    event.preventDefault();
    event.stopPropagation();
    capturing = false;
    save({ binding: { device: "mouse", button: event.button } });
  }

  const simulated = ["match_start", "normal", "rare", "spirit", "eliminated", "victory", "lobby"];
</script>

<svelte:window
  onkeydown={captureKey}
  onmousedowncapture={captureMouse}
  oncontextmenu={(event) => event.preventDefault()}
/>

{#snippet chips(c: Counts)}
  <div class="chips">
    <span class="chip tesoro">{t.treasure} <b>{c.normal}</b></span>
    <span class="chip raro">{t.rare} <b>{c.rare}</b></span>
    <span class="chip espiritu">{t.spirit} <b>{c.spirit}</b></span>
  </div>
{/snippet}

<main>
  {#if settings && !settings.setupDone}
    <h2>{t.setupTitle}</h2>
    <small class="linea">{t.setupHelp}</small>
    <form class="inicio" onsubmit={(e) => (e.preventDefault(), finishSetup())}>
      {#each ["normal", "rare", "spirit"] as const as kind}
        <label>
          {t.setupChests(t.kinds[kind])}
          <input type="number" min="0" step="1" bind:value={initial[kind]} />
        </label>
      {/each}
      <label>
        {t.setupMatches}
        <input type="number" min="0" step="1" bind:value={initial.matches} />
      </label>
      <nav>
        <button type="submit">{t.setupStart}</button>
      </nav>
    </form>
  {:else}
  {#if update}
    <div class="aviso" role="status">
      <span>{t.updateAvailable(update)}</span>
      <span class="botones">
        <button class="principal" onclick={() => invoke("open_releases").catch((e) => (error = String(e)))}>
          {t.download}
        </button>
        <button class="cerrar" aria-label={t.close} onclick={() => (update = null)}>×</button>
      </span>
    </div>
  {/if}

  <div class="tabs">
    {#each (["counter", "stats", "history", "settings", "diag"] as const).filter((name) => name !== "diag" || settings?.diagnostics) as name}
      <button class:actual={tab === name} onclick={() => (tab = name)}>{t.tabs[name]}</button>
    {/each}
  </div>

  {#if tab === "counter" && stats}
    <header class:activo={stats.inMatch}>
      <i></i>{stats.inMatch ? t.inMatch : stats.sessionActive ? t.sessionActive : t.noSession}
    </header>

    <section>
      <div class="fila"><span>{t.match}</span><strong>{stats.matchCounts.total}</strong></div>
      {@render chips(stats.matchCounts)}
    </section>

    <section>
      <div class="fila">
        <span>{t.today} <small>{elapsed(stats.sessionStartedAt)}</small></span>
        <strong>{stats.sessionCounts.total}</strong>
      </div>
      {@render chips(stats.sessionCounts)}
      <small class="linea">{t.matches}: {stats.sessionMatches}</small>
    </section>

    <section>
      <div class="fila"><span>{t.allTime}</span><strong>{stats.totalCounts.total}</strong></div>
      {@render chips(stats.totalCounts)}
      <small class="linea">{t.matches}: {stats.totalMatches}</small>
    </section>

    <nav>
      <button onclick={() => run("start_session")} disabled={stats.sessionActive}>{t.start}</button>
      <button onclick={() => run("stop_session")} disabled={!stats.sessionActive}>{t.stop}</button>
      <button onclick={() => run("new_match")}>{t.newMatch}</button>
    </nav>
  {/if}

  {#if tab === "stats" && stats}
    {@const games = stats.totalMatches}
    {@const average = (n: number) => (games > 0 ? (n / games).toFixed(2) : "—")}
    <table>
      <thead>
        <tr><th></th><th>{t.total}</th><th>{t.perMatch}</th></tr>
      </thead>
      <tbody>
        {#each ["normal", "rare", "spirit"] as const as kind}
          <tr>
            <td>{t.kinds[kind]}</td>
            <td>{stats.totalCounts[kind]}</td>
            <td>{average(stats.totalCounts[kind])}</td>
          </tr>
        {/each}
        <tr class="suma">
          <td>{t.allChests}</td>
          <td>{stats.totalCounts.total}</td>
          <td>{average(stats.totalCounts.total)}</td>
        </tr>
      </tbody>
    </table>
    <div class="fila"><span>{t.matchesPlayed}</span><strong>{games}</strong></div>
  {/if}

  {#if tab === "history"}
    <section>
      <div class="fila">
        <span>{t.historyTitle}</span>
        {#if history?.length}
          <small>{t.historySummary(history.length, history.reduce((sum, m) => sum + m.counts.total, 0))}</small>
        {/if}
      </div>
    </section>
    {#if history && history.length === 0}
      <p class="vacio">{t.historyEmpty}</p>
    {:else if history}
      <ul class="historial">
        {#each history as match (match.id)}
          <li>
            <div class="cabecera">
              <span class="duracion" title={t.duration}>{clock(match.durationMs)}</span>
              <strong>{match.counts.total}</strong>
            </div>
            {@render chips(match.counts)}
          </li>
        {/each}
      </ul>
    {/if}
  {/if}

  {#if tab === "settings" && settings}
    <section>
      <div class="opcion">
        <span>{t.language}</span>
        <select value={lang} onchange={(e) => save({ language: e.currentTarget.value })}>
          <option value="es">Español</option>
          <option value="en">English</option>
        </select>
      </div>
    </section>

    <section>
      <div class="fila"><span>{t.overlayShows}</span></div>
      <label class="opcion">
        <span>{t.showOverlay}<Help text={t.help.showOverlay} /></span>
        <input
          type="checkbox"
          checked={settings.showOverlay}
          onchange={(e) => save({ showOverlay: e.currentTarget.checked })}
        />
      </label>
      <div class="opcion">
        <span>{t.corner}</span>
        <select value={settings.corner} onchange={(e) => save({ corner: e.currentTarget.value })}>
          {#each Object.entries(t.corners) as [value, name]}
            <option {value}>{name}</option>
          {/each}
        </select>
      </div>
      <div class="opcion">
        <span>{t.optMatch}<Help text={t.help.optMatch} /></span>
        <select value={settings.overlayMatch} onchange={(e) => save({ overlayMatch: e.currentTarget.value })}>
          <option value="off">{t.optOff}</option>
          <option value="day">{t.optDay}</option>
          <option value="total">{t.optAllTime}</option>
        </select>
      </div>
      <label class="opcion">
        <span>{t.optTotal}</span>
        <input
          type="checkbox"
          checked={settings.overlayTotal}
          onchange={(e) => save({ overlayTotal: e.currentTarget.checked })}
        />
      </label>
      <div class="opcion">
        <span>{t.optChests}<Help text={t.help.optChests} /></span>
        <select value={settings.overlayChests} onchange={(e) => save({ overlayChests: e.currentTarget.value })}>
          <option value="off">{t.optOff}</option>
          <option value="sum">{t.optSum}</option>
          <option value="types">{t.optTypes}</option>
        </select>
      </div>
      <label class="opcion">
        <span>{t.optColor}</span>
        <input
          type="checkbox"
          checked={settings.overlayColor}
          onchange={(e) => save({ overlayColor: e.currentTarget.checked })}
        />
      </label>
      <div class="opcion">
        <span>{t.optSize}</span>
        <select value={settings.overlaySize} onchange={(e) => save({ overlaySize: e.currentTarget.value })}>
          {#each Object.entries(t.sizes) as [value, name]}
            <option {value}>{name}</option>
          {/each}
        </select>
      </div>
      <div class="opcion">
        <span>{t.optOpacity}</span>
        <select
          value={String(settings.overlayOpacity)}
          onchange={(e) => save({ overlayOpacity: Number(e.currentTarget.value) })}
        >
          {#each [100, 80, 60, 40] as value}
            <option value={String(value)}>{value} %</option>
          {/each}
        </select>
      </div>
      {#if stats}
        <small class="linea">{t.preview}</small>
        <div class="muestra">
          <OverlayText {stats} options={settings} {t} right={settings.corner.endsWith("right")} />
        </div>
      {/if}
      <label class="opcion">
        <span>{t.hideFromCapture}<Help text={t.help.hideFromCapture} /></span>
        <input
          type="checkbox"
          checked={settings.hideFromCapture}
          onchange={(e) => save({ hideFromCapture: e.currentTarget.checked })}
        />
      </label>
    </section>

    <section>
      <div class="opcion">
        <span>{t.openWith}<Help text={t.help.openWith} /></span>
        <b>{capturing ? t.pressNow : bindingLabel(settings.binding)}</b>
      </div>
      <nav>
        <button onclick={startCapture} disabled={capturing}>{t.changeKey}</button>
        <select
          value={settings.binding.device === "pad" ? String(settings.binding.button) : ""}
          onchange={(e) => {
            const button = e.currentTarget.value;
            if (button !== "") save({ binding: { device: "pad", button: Number(button) } });
          }}
        >
          <option value="">{t.padChoose}</option>
          {#each t.padButtons as name, button}
            <option value={String(button)}>{name}</option>
          {/each}
        </select>
      </nav>
      <label class="opcion">
        <span>{t.requireInput}<Help text={t.help.requireInput} /></span>
        <input
          type="checkbox"
          checked={settings.requireInput}
          onchange={(e) => save({ requireInput: e.currentTarget.checked })}
        />
      </label>
    </section>

    <section>
      <div class="fila"><span>{t.advanced}</span></div>
      <label class="opcion">
        <span>{t.checkUpdates}<Help text={t.help.checkUpdates} /></span>
        <input
          type="checkbox"
          checked={settings.checkUpdates}
          onchange={(e) => save({ checkUpdates: e.currentTarget.checked })}
        />
      </label>
      <label class="opcion">
        <span>{t.diagnosticsToggle}<Help text={t.help.diagnostics} /></span>
        <input
          type="checkbox"
          checked={settings.diagnostics}
          onchange={(e) => save({ diagnostics: e.currentTarget.checked })}
        />
      </label>
    </section>

    <section>
      <div class="opcion">
        <span>{t.resetTitle}<Help text={t.resetHelp} /></span>
        {#if confirmingReset}
          <span class="botones">
            <button
              class="peligro"
              onclick={() => {
                confirmingReset = false;
                initial = { normal: 0, rare: 0, spirit: 0, matches: 0 };
                save({ setupDone: false });
              }}
            >
              {t.resetConfirm}
            </button>
            <button onclick={() => (confirmingReset = false)}>{t.cancel}</button>
          </span>
        {:else}
          <button onclick={() => (confirmingReset = true)}>{t.resetTitle}</button>
        {/if}
      </div>
    </section>
  {/if}

  {#if tab === "diag" && diag}
    <section>
      <div class="fila"><span>{t.detection}</span></div>
      {#if diag.detectError}
        <small class="linea error">{t.stopped}: {diag.detectError}</small>
      {:else}
        <small class="linea">
          {t.active} · {t.countedSinceOpen}: {diag.detected}
          {#if diag.lastDetected}
            · {t.last}: {t.kindNames[diag.lastDetected] ?? diag.lastDetected}
          {/if}
        </small>
        <small class="linea">{t.withInput}: {diag.detectedWithInput} / {diag.detected}</small>
        <small class="linea">{t.matchHud}: <b>{diag.hudVisible ? t.visible : t.notVisible}</b></small>
      {/if}
      {#if settings}
        <label class="opcion">
          <span>{t.saveSamples}<Help text={t.help.saveSamples} /></span>
          <input
            type="checkbox"
            checked={settings.saveSamples}
            onchange={(e) => save({ saveSamples: e.currentTarget.checked })}
          />
        </label>
      {/if}
    </section>

    <section>
      <div class="fila"><span>{t.diagnostics}</span></div>
      {#if !diag.supported}
        <small class="linea error">{t.windowsOnly}</small>
      {/if}
      <small class="linea">
        {t.openButton}: <b>{diag.inputDown ? t.pressed : t.released}</b> · {t.pressesSeen}: {diag.inputPresses}
      </small>
      <small class="linea">
        {t.hotkeysPolled}: {diag.pollHits} · {t.hotkeysGlobal}: {diag.hotkeyHits}
      </small>
      {#if diag.hotkeyError}
        <small class="linea error">{t.hotkeyUnavailable}: {diag.hotkeyError}</small>
      {/if}
      <nav>
        <button onclick={() => invoke("diag_shot")}>{t.shotIn5}</button>
        <button onclick={() => invoke("diag_burst")}>
          {diag.recording ? t.stopBurst : t.burstIn5}
        </button>
        <button onclick={() => invoke("open_captures_dir").catch((e) => (error = String(e)))}>
          {t.openFolder}
        </button>
      </nav>
      {#if diag.pendingIn}<small class="linea">{t.startsIn(diag.pendingIn)}</small>{/if}
      {#if diag.recording}<small class="linea">{t.recording(diag.frames)}</small>{/if}
      {#if diag.lastResult}
        <small class="linea" class:error={!diag.lastResult.ok}>{diag.lastResult.message}</small>
      {/if}
      {#if thumbnail}<img src={thumbnail} alt={t.lastCapture} />{/if}
      <small class="linea">{diag.dir}</small>
    </section>

    {#if debug}
      <nav class="debug">
        {#each simulated as event}
          <button onclick={() => run("debug_simulate", { event })}>{event}</button>
        {/each}
      </nav>
    {/if}
  {/if}

  {/if}

  {#if error}<p class="error">{error}</p>{/if}
</main>

<style>
  /* Paleta "Gaming" de ui-ux-pro-max: violeta neón sobre fondo muy oscuro.
     Los colores van como variables; los componentes no usan hex sueltos. */
  :global(:root) {
    --bg: #0f0f23;
    --card: #1e1c35;
    --muted: #27273b;
    --border: #3b2a6b;
    --text: #e2e8f0;
    --text-muted: #94a3b8;
    --primary: #7c3aed;
    --primary-soft: #a78bfa;
    --accent: #f43f5e;
    --danger: #f87171;
    --ok: #4ade80;
    /* Un color por tipo de contenedor, parecido al del juego. */
    --tesoro: #fbbf24;
    --raro: #60a5fa;
    --espiritu: #e879f9;
  }

  :global(html, body) {
    margin: 0;
    background: var(--bg);
    color: var(--text);
    font-family: "Segoe UI", system-ui, sans-serif;
    font-size: 15px;
    line-height: 1.5;
    user-select: none;
  }

  main {
    display: flex;
    flex-direction: column;
    gap: 12px;
    box-sizing: border-box;
    width: 100%;
    max-width: 620px;
    margin: 0 auto;
    padding: 12px clamp(10px, 4vw, 20px) 20px;
  }

  .tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 2px;
    border-bottom: 1px solid var(--muted);
  }

  .tabs button {
    border: none;
    border-bottom: 2px solid transparent;
    border-radius: 6px 6px 0 0;
    background: none;
    color: var(--text-muted);
    padding: 8px 9px;
    font-size: 14px;
  }

  .tabs button:hover {
    color: var(--text);
    background: var(--muted);
  }

  .tabs button.actual {
    color: var(--text);
    border-bottom-color: var(--primary-soft);
  }

  header {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.08em;
    color: var(--text-muted);
  }

  header i {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--text-muted);
  }

  header.activo {
    color: var(--ok);
  }

  header.activo i {
    background: var(--ok);
    box-shadow: 0 0 8px var(--ok);
  }

  section {
    background: var(--card);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 12px 14px;
  }

  .fila {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.08em;
    color: var(--text-muted);
  }

  strong {
    font-size: 30px;
    line-height: 1.1;
    letter-spacing: 0;
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }

  small {
    color: var(--text-muted);
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }

  .linea {
    display: block;
    overflow-wrap: anywhere;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 8px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 2px 9px;
    border-radius: 999px;
    background: var(--muted);
    font-size: 13px;
    color: var(--text-muted);
  }

  .chip::before {
    content: "";
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: currentColor;
  }

  .chip b {
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }

  .chip.tesoro::before {
    background: var(--tesoro);
  }

  .chip.raro::before {
    background: var(--raro);
  }

  .chip.espiritu::before {
    background: var(--espiritu);
  }

  /* Vista previa del contador sobre un fondo parecido al del juego. */
  .muestra {
    margin: 6px 0 10px;
    padding: 0 0 38px;
    border-radius: 8px;
    background: linear-gradient(135deg, #3b82c4, #7fb7d9 45%, #c9a45c);
    overflow: hidden;
  }

  nav {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  section nav {
    margin: 8px 0;
  }

  button,
  select,
  input {
    min-height: 36px;
    box-sizing: border-box;
    background: var(--muted);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 6px 12px;
    font: inherit;
  }

  button {
    cursor: pointer;
    transition: background 150ms ease-out;
  }

  button:hover:not(:disabled) {
    background: var(--primary);
    border-color: var(--primary);
  }

  button.peligro {
    border-color: var(--danger);
    color: var(--danger);
  }

  button.peligro:hover:not(:disabled) {
    background: var(--danger);
    border-color: var(--danger);
    color: var(--bg);
  }

  button:disabled {
    opacity: 0.45;
    cursor: default;
  }

  button:focus-visible,
  select:focus-visible,
  input:focus-visible {
    outline: 2px solid var(--primary-soft);
    outline-offset: 2px;
  }

  button[type="submit"] {
    background: var(--primary);
    border-color: var(--primary);
    font-weight: 600;
  }

  input[type="checkbox"] {
    min-height: 0;
    width: 18px;
    height: 18px;
    padding: 0;
    accent-color: var(--primary);
    flex-shrink: 0;
  }

  input[type="number"] {
    width: 110px;
  }

  /* Una opción por fila: nombre a la izquierda, control a la derecha. En
     ventanas estrechas el control baja a la línea siguiente. */
  .opcion {
    position: relative;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 6px 12px;
    min-height: 40px;
    font-size: 14px;
  }

  .opcion > span:first-child {
    display: inline-flex;
    align-items: center;
  }

  .opcion select {
    max-width: 100%;
  }

  .botones {
    display: inline-flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  h2 {
    margin: 0;
    font-size: 20px;
  }

  form {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .inicio label {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    font-size: 14px;
  }

  /* Aviso de versión nueva: se queda hasta que se cierra, porque lleva una acción. */
  .aviso {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px 12px;
    padding: 10px 12px;
    border: 1px solid var(--primary);
    border-radius: 10px;
    background: color-mix(in srgb, var(--primary) 22%, var(--card));
    font-size: 14px;
    font-weight: 600;
  }

  button.principal {
    background: var(--primary);
    border-color: var(--primary);
    font-weight: 600;
  }

  button.cerrar {
    min-width: 36px;
    padding: 0 10px;
    font-size: 18px;
    line-height: 1;
  }

  .historial {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .historial li {
    background: var(--card);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 10px 14px;
  }

  .cabecera {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
  }

  .duracion {
    font-size: 16px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: var(--text);
  }

  .historial strong {
    font-size: 24px;
  }

  .vacio {
    margin: 4px 0;
    color: var(--text-muted);
    font-size: 14px;
    text-align: center;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-variant-numeric: tabular-nums;
  }

  th,
  td {
    padding: 8px 4px;
    text-align: right;
    border-bottom: 1px solid var(--muted);
  }

  th {
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.06em;
    color: var(--text-muted);
  }

  td:first-child {
    text-align: left;
  }

  .suma td {
    font-weight: 700;
    border-bottom: none;
    color: var(--primary-soft);
  }

  img {
    display: block;
    max-width: 100%;
    margin: 8px 0;
    border: 1px solid var(--border);
    border-radius: 6px;
  }

  .debug button {
    font-size: 12px;
    border-style: dashed;
  }

  .error {
    color: var(--danger);
    margin: 0;
  }

  /* Ventanas muy estrechas: menos relleno y números algo más pequeños. */
  @media (max-width: 380px) {
    section {
      padding: 10px;
    }

    strong {
      font-size: 26px;
    }

    .tabs button {
      padding: 8px 7px;
      font-size: 13px;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    button {
      transition: none;
    }
  }
</style>
