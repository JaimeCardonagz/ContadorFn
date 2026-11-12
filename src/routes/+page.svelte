<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { texts, type Lang } from "$lib/i18n";

  type Counts = { normal: number; rare: number; spirit: number; ammo: number; total: number };
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
  };

  let stats = $state<Stats | null>(null);
  let diag = $state<Diag | null>(null);
  let settings = $state<Settings | null>(null);
  let thumbnail = $state("");
  let debug = $state(false);
  let error = $state("");
  let now = $state(Date.now());
  let tab = $state<"counter" | "stats" | "settings" | "history" | "diag">("counter");

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
    });
    const listeners = [
      listen<Stats>("stats-changed", (event) => (stats = event.payload)),
      listen<Diag>("diag-state", (event) => (diag = event.payload)),
      listen<string>("diag-thumbnail", (event) => (thumbnail = event.payload)),
    ];
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => {
      for (const listener of listeners) listener.then((stop) => stop());
      clearInterval(timer);
    };
  });

  function breakdown(c: Counts) {
    return `${t.treasure} ${c.normal} · ${t.rare} ${c.rare} · ${t.spirit} ${c.spirit} · ${t.ammo} ${c.ammo}`;
  }

  function elapsed(since: number | null) {
    if (since === null) return "";
    const s = Math.max(0, Math.floor((now - since) / 1000));
    const pad = (n: number) => String(n).padStart(2, "0");
    return `${pad(Math.floor(s / 3600))}:${pad(Math.floor(s / 60) % 60)}:${pad(s % 60)}`;
  }

  // Ajuste manual del histórico: cofres abiertos antes de usar la app, o correcciones.
  let adjustType = $state("normal");
  let adjustAmount = $state<number | null>(null);

  async function adjust() {
    if (!adjustAmount || !Number.isInteger(adjustAmount)) return;
    if (adjustType === "matches") {
      await run("adjust_matches", { delta: adjustAmount });
    } else {
      await run("adjust_total", { chest: adjustType, delta: adjustAmount });
    }
    if (!error) adjustAmount = null;
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

  const simulated = ["match_start", "normal", "rare", "spirit", "ammo", "eliminated", "victory", "lobby"];
</script>

<svelte:window
  onkeydown={captureKey}
  onmousedowncapture={captureMouse}
  oncontextmenu={(event) => event.preventDefault()}
/>

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
  <div class="tabs">
    {#each ["counter", "stats", "settings", "history", "diag"] as const as name}
      <button class:actual={tab === name} onclick={() => (tab = name)}>{t.tabs[name]}</button>
    {/each}
  </div>

  {#if tab === "counter" && stats}
    <header class:activo={stats.inMatch}>
      ● {stats.inMatch ? t.inMatch : stats.sessionActive ? t.sessionActive : t.noSession}
    </header>

    <section>
      <div class="fila"><span>{t.match}</span><strong>{stats.matchCounts.total}</strong></div>
      <small>{breakdown(stats.matchCounts)}</small>
    </section>

    <section>
      <div class="fila">
        <span>{t.today} <small>{elapsed(stats.sessionStartedAt)}</small></span>
        <strong>{stats.sessionCounts.total}</strong>
      </div>
      <small>{breakdown(stats.sessionCounts)}</small>
      <small class="linea">{t.matches}: {stats.sessionMatches}</small>
    </section>

    <section>
      <div class="fila"><span>{t.allTime}</span><strong>{stats.totalCounts.total}</strong></div>
      <small>{breakdown(stats.totalCounts)}</small>
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
        {#each ["normal", "rare", "spirit", "ammo"] as const as kind}
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

  {#if tab === "settings" && settings}
    <label class="ajuste">
      {t.language}
      <select value={lang} onchange={(e) => save({ language: e.currentTarget.value })}>
        <option value="es">Español</option>
        <option value="en">English</option>
      </select>
    </label>

    <section>
      <label class="ajuste">
        <input
          type="checkbox"
          checked={settings.showOverlay}
          onchange={(e) => save({ showOverlay: e.currentTarget.checked })}
        />
        {t.showOverlay}
      </label>
      <label class="ajuste">
        {t.corner}
        <select
          value={settings.corner}
          disabled={!settings.showOverlay}
          onchange={(e) => save({ corner: e.currentTarget.value })}
        >
          {#each Object.entries(t.corners) as [value, name]}
            <option {value}>{name}</option>
          {/each}
        </select>
      </label>
      <label class="ajuste">
        <input
          type="checkbox"
          checked={settings.hideFromCapture}
          onchange={(e) => save({ hideFromCapture: e.currentTarget.checked })}
        />
        {t.hideFromCapture}
      </label>
    </section>

    <section>
      <div class="ajuste">
        {t.openWith}: <b>{capturing ? t.pressNow : bindingLabel(settings.binding)}</b>
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
      <label class="ajuste">
        <input
          type="checkbox"
          checked={settings.requireInput}
          onchange={(e) => save({ requireInput: e.currentTarget.checked })}
        />
        {t.requireInput}
      </label>
    </section>
  {/if}

  {#if tab === "history"}
    <small class="linea">{t.historyHelp}</small>
    <form onsubmit={(e) => (e.preventDefault(), adjust())}>
      <select bind:value={adjustType}>
        <option value="normal">{t.kinds.normal}</option>
        <option value="rare">{t.kinds.rare}</option>
        <option value="spirit">{t.kinds.spirit}</option>
        <option value="ammo">{t.kinds.ammo}</option>
        <option value="matches">{t.matches}</option>
      </select>
      <input type="number" step="1" placeholder={t.amount} bind:value={adjustAmount} />
      <button type="submit" disabled={!adjustAmount}>{t.apply}</button>
    </form>
    {#if stats}
      <section>
        <div class="fila"><span>{t.allTime}</span><strong>{stats.totalCounts.total}</strong></div>
        <small>{breakdown(stats.totalCounts)}</small>
        <small class="linea">{t.matches}: {stats.totalMatches}</small>
      </section>
    {/if}
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

  .linea {
    display: block;
    overflow-wrap: anywhere;
  }

  img {
    display: block;
    max-width: 100%;
    margin: 6px 0;
    border: 1px solid #3a4050;
  }

  .ajuste {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 6px 0;
    font-size: 13px;
  }

  .ajuste input {
    width: auto;
  }

  h2 {
    margin: 0;
    font-size: 18px;
  }

  .inicio {
    flex-direction: column;
    align-items: stretch;
    border-top: none;
  }

  .inicio label {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-variant-numeric: tabular-nums;
  }

  th,
  td {
    padding: 6px 4px;
    text-align: right;
    border-bottom: 1px solid #2a2e3a;
  }

  th {
    font-size: 12px;
    font-weight: normal;
    color: #9aa1b2;
  }

  td:first-child {
    text-align: left;
  }

  .suma td {
    font-weight: bold;
  }

  .tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    border-bottom: 1px solid #2a2e3a;
    padding-bottom: 8px;
  }

  .tabs button {
    border: none;
    background: none;
    color: #9aa1b2;
  }

  .tabs button.actual {
    color: #e8eaf0;
    background: #262a36;
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
