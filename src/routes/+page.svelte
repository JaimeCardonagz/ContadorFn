<script lang="ts">
  import { appVersion, invoke, listen } from "$lib/tauri";
  import { onMount } from "svelte";
  import { texts, type Lang } from "$lib/i18n";
  import OverlayText from "$lib/OverlayText.svelte";
  import Help from "$lib/Help.svelte";
  import { clock } from "$lib/format";

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
    lastChest: { kind: string; at: number } | null;
    paused: boolean;
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
    gameWidth: number;
    gameHeight: number;
    gameWindow: boolean;
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
    closeToTray: boolean;
    startWithWindows: boolean;
    matchSummary: boolean;
    overlayHotkey: string;
    pauseHotkey: string;
    overlayOnlyInMatch: boolean;
    overlayFlash: boolean;
    dailyGoal: number;
    overlayGoal: boolean;
    theme: string;
    lastSeenVersion: string;
    overlayX: number;
    overlayY: number;
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
  type Summary = { id: number; startedAt: number; durationMs: number; counts: Counts };
  let history = $state<Summary[] | null>(null);
  const loadHistory = () =>
    invoke<Summary[]>("get_history")
      .then((value) => (history = value))
      .catch((e) => (error = String(e)));
  $effect(() => {
    if (tab === "history") loadHistory();
  });

  // Récords: la partida con más cofres, la más larga y el promedio de 24 h.
  type RecordMatch = { startedAt: number; durationMs: number; counts: Counts };
  type Records = {
    mostChests: RecordMatch | null;
    longest: RecordMatch | null;
    dayMatches: number;
    dayChests: number;
  };
  let records = $state<Records | null>(null);
  const loadRecords = () =>
    invoke<Records>("get_records")
      .then((value) => (records = value))
      .catch((e) => (error = String(e)));
  $effect(() => {
    if (tab === "stats") loadRecords();
  });
  const day = (ms: number) =>
    new Date(ms).toLocaleDateString(lang === "en" ? "en-US" : "es-MX", { day: "numeric", month: "short" });

  // Pestañas: flechas izquierda/derecha, Inicio y Fin entre ellas (como en
  // cualquier lista de pestañas) y Ctrl+1…5 desde cualquier sitio.
  const visibleTabs = $derived(
    (["counter", "stats", "history", "settings", "diag"] as const).filter((name) => name !== "diag" || settings?.diagnostics),
  );
  function goToTab(index: number, focus = true) {
    const list = visibleTabs;
    const next = list[(index + list.length) % list.length];
    tab = next;
    if (focus) setTimeout(() => document.getElementById(`tab-${next}`)?.focus(), 0);
  }
  function tabKey(event: KeyboardEvent) {
    const current = visibleTabs.indexOf(tab);
    const moves: Record<string, number> = { ArrowRight: current + 1, ArrowLeft: current - 1, Home: 0, End: visibleTabs.length - 1 };
    if (event.key in moves) {
      event.preventDefault();
      goToTab(moves[event.key]);
    }
  }
  function globalKey(event: KeyboardEvent) {
    if (capturing || capturingHotkey) return;
    if (event.ctrlKey && !event.altKey && /^[1-5]$/.test(event.key)) {
      const index = Number(event.key) - 1;
      if (index < visibleTabs.length) {
        event.preventDefault();
        goToTab(index);
      }
    }
  }

  // Tema del panel: oscuro, claro o el de Windows.
  let systemLight = $state(false);
  $effect(() => {
    const query = window.matchMedia("(prefers-color-scheme: light)");
    systemLight = query.matches;
    const change = (e: MediaQueryListEvent) => (systemLight = e.matches);
    query.addEventListener("change", change);
    return () => query.removeEventListener("change", change);
  });
  $effect(() => {
    const choice = settings?.theme ?? "dark";
    const light = choice === "light" || (choice === "system" && systemLight);
    document.documentElement.dataset.theme = light ? "light" : "dark";
  });

  // Sin modo diagnóstico no hay pestaña Diagnóstico.
  $effect(() => {
    if (tab === "diag" && settings && !settings.diagnostics) tab = "counter";
  });

  // Aviso de versión nueva.
  let version = $state("");
  let update = $state<string | null>(null);
  let updating = $state(false);
  let updateFailed = $state(false);

  async function installUpdate() {
    updating = true;
    updateFailed = false;
    try {
      // Si va bien, el programa se cierra y se abre la versión nueva.
      await invoke("install_update");
    } catch {
      updateFailed = true;
    } finally {
      updating = false;
    }
  }

  // Idioma de la interfaz: el guardado o, la primera vez, el del sistema.
  const lang = $derived<Lang>(
    settings?.language === "en" || settings?.language === "es"
      ? settings.language
      : navigator.language.toLowerCase().startsWith("es")
        ? "es"
        : "en",
  );
  const t = $derived(texts[lang]);

  const locale = $derived(lang === "en" ? "en-US" : "es-MX");
  const hourOf = (ms: number) => new Date(ms).toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" });

  // ¿Está el programa analizando el juego o esperando a que esté delante?
  let waiting = $state(true);

  // Copiar al portapapeles un resumen del día, para compartirlo.
  let copied = $state(false);
  async function copySummary() {
    if (!stats) return;
    const c = stats.sessionCounts;
    const text = t.copyText(c.total, c.normal, c.rare, c.spirit, stats.sessionMatches, stats.totalCounts.total);
    try {
      await navigator.clipboard.writeText(text);
      copied = true;
      setTimeout(() => (copied = false), 2000);
    } catch (e) {
      error = String(e);
    }
  }

  // Últimos 7 días (Estadísticas) y exportar partidas (Historial).
  let allMatches = $state<Summary[] | null>(null);
  const loadAllMatches = () =>
    invoke<Summary[]>("get_all_matches")
      .then((value) => (allMatches = value))
      .catch((e) => (error = String(e)));
  $effect(() => {
    if (tab === "stats") loadAllMatches();
  });
  const dayKey = (ms: number) => {
    const d = new Date(ms);
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
  };
  const lastDays = $derived.by(() => {
    const days: { key: string; label: string; matches: number; chests: number }[] = [];
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    for (let i = 0; i < 7; i++) {
      const date = new Date(today);
      date.setDate(today.getDate() - i);
      const label = i === 0 ? t.today2 : i === 1 ? t.yesterday : date.toLocaleDateString(locale, { weekday: "short", day: "numeric", month: "short" });
      days.push({ key: dayKey(date.getTime()), label, matches: 0, chests: 0 });
    }
    for (const match of allMatches ?? []) {
      const day = days.find((d) => d.key === dayKey(match.startedAt));
      if (day) {
        day.matches += 1;
        day.chests += match.counts.total;
      }
    }
    return days.filter((d) => d.matches > 0);
  });

  // Restaurar una copia de seguridad.
  type BackupFile = { name: string; kind: string; day: string | null };
  let backupFiles = $state<BackupFile[] | null>(null);
  let restoreChoice = $state("");
  let restoreDone = $state(false);
  async function openRestore() {
    restoreDone = false;
    backupFiles = await invoke<BackupFile[]>("list_backups").catch(() => []);
    restoreChoice = backupFiles[0]?.name ?? "";
  }
  async function restoreBackup() {
    if (!restoreChoice) return;
    try {
      stats = await invoke<Stats>("restore_backup", { name: restoreChoice });
      backupFiles = null;
      restoreDone = true;
      error = "";
      invoke<BackupInfo>("backup_info").then((value) => (backups = value)).catch(() => {});
    } catch (e) {
      error = String(e);
    }
  }
  const backupLabel = (file: BackupFile) =>
    file.kind === "daily" && file.day ? backupDay(file.day) : file.kind === "reset" ? t.backupBeforeReset : t.backupBeforeRestore;

  // Novedades: tras actualizar, una vez por versión.
  const showWhatsNew = $derived(
    !!settings && settings.setupDone && version !== "" && settings.lastSeenVersion !== version,
  );


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
    invoke<Problem>("get_vision_problem")
      .then((value) => (problem = value))
      .catch(() => {});
    invoke<boolean>("get_detection_waiting")
      .then((value) => (waiting = value))
      .catch(() => {});
    // El programa puede pasar días en la bandeja: se vuelve a mirar si hay
    // versión nueva cada 6 horas.
    const updateTimer = setInterval(
      () => {
        if (settings?.checkUpdates && !update) {
          invoke<string | null>("check_update")
            .then((value) => (update = value))
            .catch(() => {});
        }
      },
      6 * 60 * 60 * 1000,
    );
    appVersion().then((value) => (version = value));
    invoke<boolean>("is_debug").then((value) => (debug = value));
    invoke<Diag>("get_diag").then((value) => (diag = value));
    invoke<Settings>("get_settings").then((value) => {
      settings = value;
      // La primera vez se guarda el idioma del sistema, para que el backend
      // genere sus mensajes en el mismo idioma que la interfaz.
      if (!value.language) save({ language: lang });
      if (value.checkUpdates) {
        invoke<string | null>("check_update")
          .then((version) => (update = version))
          .catch(() => {});
      }
    });
    const listeners = [
      listen<Stats>("stats-changed", (event) => {
        stats = event.payload;
        if (tab === "history") loadHistory();
        if (tab === "stats") loadRecords();
      }),
      // El atajo global cambia "Mostrar contador" sin pasar por el panel.
      listen<Settings>("settings-changed", (event) => (settings = event.payload)),
      listen<Problem>("vision-problem", (event) => (problem = event.payload)),
      listen<boolean>("detection-waiting", (event) => (waiting = event.payload)),
      listen<boolean>("overlay-moving", (event) => (movingOverlay = event.payload)),
      listen<Diag>("diag-state", (event) => (diag = event.payload)),
      listen<string>("diag-thumbnail", (event) => (thumbnail = event.payload)),
    ];
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => {
      for (const listener of listeners) listener.then((stop) => stop());
      clearInterval(timer);
      clearInterval(updateTimer);
    };
  });

  function elapsed(since: number | null) {
    if (since === null) return "";
    const s = Math.max(0, Math.floor((now - since) / 1000));
    const pad = (n: number) => String(n).padStart(2, "0");
    return `${pad(Math.floor(s / 3600))}:${pad(Math.floor(s / 60) % 60)}:${pad(s % 60)}`;
  }

  // Primera vez (o al reiniciar): lo que el jugador ya llevaba. Dos pasos:
  // escribir los números (validados) y revisar el resumen antes de borrar.
  type SetupField = "normal" | "rare" | "spirit" | "matches";
  const SETUP_MAX = 1_000_000;
  let initial = $state<Record<SetupField, number | null>>({ normal: 0, rare: 0, spirit: 0, matches: 0 });
  let setupStep = $state<"form" | "review">("form");

  /** Error de un campo, o "" si vale. Vacío cuenta como 0. */
  function setupError(value: number | null): string {
    if (value === null || value === undefined || Number.isNaN(value)) return "";
    if (!Number.isInteger(value) || value < 0) return t.setupInvalid;
    if (value > SETUP_MAX) return t.setupTooBig;
    return "";
  }
  const setupValue = (value: number | null) => (value === null || Number.isNaN(value) ? 0 : value);
  const setupValid = $derived((Object.values(initial) as (number | null)[]).every((v) => setupError(v) === ""));
  const setupTotal = $derived(setupValue(initial.normal) + setupValue(initial.rare) + setupValue(initial.spirit));
  // Al reiniciar hay datos que se van a borrar: se enseñan en el resumen.
  const hadData = $derived(!!stats && (stats.totalCounts.total > 0 || stats.totalMatches > 0));

  function reviewSetup() {
    if (setupValid) setupStep = "review";
  }

  async function finishSetup() {
    await run("initial_setup", {
      normal: setupValue(initial.normal),
      rare: setupValue(initial.rare),
      spirit: setupValue(initial.spirit),
      matches: setupValue(initial.matches),
    });
    if (!error) {
      await save({ setupDone: true, lastSeenVersion: settings?.lastSeenVersion || version });
      setupStep = "form";
    }
  }

  /** Al reiniciar, volver atrás sin borrar nada. */
  async function cancelReset() {
    setupStep = "form";
    await save({ setupDone: true });
  }

  /** Enfoca el elemento al aparecer (primer campo, botón de confirmar). */
  function autofocus(node: HTMLElement) {
    // En el siguiente fotograma: al montarse, la ventana aún puede estar
    // terminando de pintar y el foco se perdería.
    setTimeout(() => node.focus(), 50);
  }

  // Deshacer el último cofre: solo si es de los últimos 5 minutos.
  const UNDO_WINDOW_MS = 5 * 60 * 1000;
  let confirmingUndo = $state(false);
  const undoable = $derived(stats?.lastChest && now - stats.lastChest.at <= UNDO_WINDOW_MS ? stats.lastChest : null);
  $effect(() => {
    if (!undoable) confirmingUndo = false;
  });

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

  // Atajo global para mostrar u ocultar el contador, como "Ctrl+Shift+KeyO".
  type HotkeyField = "overlayHotkey" | "pauseHotkey";
  let capturingHotkey = $state<HotkeyField | null>(null);
  let hotkeyHintFor = $state<HotkeyField | null>(null);
  let hotkeyHint = $state("");
  const MODIFIER_KEYS = ["Control", "Shift", "Alt", "Meta", "AltGraph"];

  /** Partes legibles del atajo: "Ctrl+Shift+KeyO" → ["Ctrl", "Shift", "O"]. */
  const hotkeyParts = (accel: string) =>
    accel.split("+").map((part) => part.replace(/^Key/, "").replace(/^Digit/, ""));

  function startHotkeyCapture(field: HotkeyField) {
    hotkeyHint = "";
    hotkeyHintFor = field;
    setTimeout(() => (capturingHotkey = field), 200);
  }

  async function captureHotkey(event: KeyboardEvent) {
    if (!capturingHotkey || !settings) return;
    const field = capturingHotkey;
    event.preventDefault();
    if (event.key === "Escape") {
      capturingHotkey = null;
      return;
    }
    if (MODIFIER_KEYS.includes(event.key)) return; // falta la tecla principal
    const fn = /^F([1-9]|1[0-2])$/.test(event.code);
    const main = /^(Key[A-Z]|Digit[0-9])$/.test(event.code) || fn;
    if (!main) {
      hotkeyHint = t.hotkeyInvalid;
      return;
    }
    // Sin Ctrl ni Alt, una letra o un número se le quitaría al juego al escribir.
    if (!fn && !event.ctrlKey && !event.altKey) {
      hotkeyHint = t.hotkeyNeedsModifier;
      return;
    }
    if (fn && ["F8", "F9"].includes(event.code) && !event.ctrlKey && !event.altKey && !event.shiftKey) {
      hotkeyHint = t.hotkeyReserved;
      return;
    }
    const accel = [
      event.ctrlKey && "Ctrl",
      event.altKey && "Alt",
      event.shiftKey && "Shift",
      event.code,
    ]
      .filter(Boolean)
      .join("+");
    capturingHotkey = null;
    await saveHotkey(field, accel);
  }

  /** Guarda el atajo; si el sistema lo rechaza (otro programa lo usa), vuelve al anterior. */
  async function saveHotkey(field: HotkeyField, accel: string) {
    if (!settings) return;
    const previous = settings[field];
    hotkeyHintFor = field;
    await save({ [field]: accel });
    if (error) {
      hotkeyHint = error;
      error = "";
      settings = { ...settings, [field]: previous };
    } else {
      hotkeyHint = "";
    }
  }

  // Copias de seguridad automáticas (una al día, las 7 últimas).
  type BackupInfo = { count: number; latest: string | null };
  let backups = $state<BackupInfo | null>(null);
  $effect(() => {
    if (tab === "settings") invoke<BackupInfo>("backup_info").then((value) => (backups = value)).catch(() => {});
  });
  /** "2026-11-18" → "18 nov", con la fecha local (sin desfase por zona horaria). */
  const backupDay = (iso: string) => {
    const [y, m, d] = iso.split("-").map(Number);
    return new Date(y, m - 1, d).toLocaleDateString(lang === "en" ? "en-US" : "es-MX", { day: "numeric", month: "short" });
  };

  // Aviso si el programa no consigue ver el juego, con una pista de qué hacer.
  type Problem = "none" | "capture" | "black" | "small";
  let problem = $state<Problem>("none");

  // Colocar el contador arrastrándolo.
  let movingOverlay = $state(false);
  async function moveOverlay(start: boolean, saveIt = false) {
    try {
      if (start) {
        await invoke("start_overlay_move");
        movingOverlay = true;
      } else {
        settings = await invoke<Settings>("finish_overlay_move", { save: saveIt });
        movingOverlay = false;
      }
      error = "";
    } catch (e) {
      error = String(e);
    }
  }

  const simulated = ["match_start", "normal", "rare", "spirit", "eliminated", "victory", "lobby"];
</script>

<svelte:window
  onkeydown={(event) => {
    globalKey(event);
    captureHotkey(event);
    captureKey(event);
  }}
  onmousedowncapture={captureMouse}
  oncontextmenu={(event) => event.preventDefault()}
/>

{#snippet hotkeyRow(field: HotkeyField, label: string, help: string)}
  {#if settings}
    <div class="opcion">
      <span>{label}<Help text={help} /></span>
      <span class="atajo" aria-live="polite">
        {#if capturingHotkey === field}
          <em>{t.hotkeyPress}</em>
        {:else if settings[field]}
          {#each hotkeyParts(settings[field]) as part, i}
            {#if i > 0}<span aria-hidden="true">+</span>{/if}<kbd>{part}</kbd>
          {/each}
        {:else}
          <em>{t.hotkeyNone}</em>
        {/if}
      </span>
    </div>
    <nav class="botones">
      <button onclick={() => startHotkeyCapture(field)} disabled={capturingHotkey !== null}>
        {settings[field] ? t.changeKey : t.hotkeyAssign}
      </button>
      {#if capturingHotkey === field}
        <button onclick={() => (capturingHotkey = null)}>{t.cancel}</button>
      {:else if settings[field]}
        <button onclick={() => saveHotkey(field, "")}>{t.hotkeyRemove}</button>
      {/if}
    </nav>
    {#if hotkeyHint && hotkeyHintFor === field}<p class="aviso-atajo" role="alert">{hotkeyHint}</p>{/if}
  {/if}
{/snippet}

{#snippet chips(c: Counts)}
  <div class="chips">
    <span class="chip tesoro">{t.treasure} <b>{c.normal}</b></span>
    <span class="chip raro">{t.rare} <b>{c.rare}</b></span>
    <span class="chip espiritu">{t.spirit} <b>{c.spirit}</b></span>
  </div>
{/snippet}

<main>
  {#if settings && !settings.setupDone}
    {#if setupStep === "form"}
      <h2>{t.setupTitle}</h2>
      <small class="linea">{t.setupHelp}</small>
      <form class="inicio" novalidate onsubmit={(e) => (e.preventDefault(), reviewSetup())}>
        {#each ["normal", "rare", "spirit", "matches"] as const as field, i}
          {@const problem = setupError(initial[field])}
          <div class="campo">
            <label>
              <span>{field === "matches" ? t.setupMatches : t.setupChests(t.kinds[field])}</span>
              {#if i === 0}
                <input
                  type="number"
                  min="0"
                  step="1"
                  inputmode="numeric"
                  placeholder="0"
                  bind:value={initial[field]}
                  aria-invalid={problem !== ""}
                  aria-describedby={problem ? `error-${field}` : undefined}
                  use:autofocus
                />
              {:else}
                <input
                  type="number"
                  min="0"
                  step="1"
                  inputmode="numeric"
                  placeholder="0"
                  bind:value={initial[field]}
                  aria-invalid={problem !== ""}
                  aria-describedby={problem ? `error-${field}` : undefined}
                />
              {/if}
            </label>
            {#if problem}<small class="error-campo" id={`error-${field}`}>{problem}</small>{/if}
          </div>
        {/each}
        <small class="linea">{t.setupEnter}</small>
        <nav class="botones">
          <button type="submit" class="principal" disabled={!setupValid}>{t.setupNext}</button>
          {#if hadData}
            <button type="button" onclick={cancelReset}>{t.setupCancelReset}</button>
          {/if}
        </nav>
      </form>
    {:else}
      <h2>{t.setupReview}</h2>
      <small class="linea">{t.setupReviewHelp}</small>
      <form class="inicio" onsubmit={(e) => (e.preventDefault(), finishSetup())}>
        <dl class="resumen">
          {#each ["normal", "rare", "spirit"] as const as kind}
            <div><dt>{t.kinds[kind]}</dt><dd>{setupValue(initial[kind])}</dd></div>
          {/each}
          <div class="suma"><dt>{t.allChests}</dt><dd>{setupTotal}</dd></div>
          <div><dt>{t.matchesPlayed}</dt><dd>{setupValue(initial.matches)}</dd></div>
        </dl>
        {#if hadData && stats}
          <p class="borrar" role="note">{t.setupErase(stats.totalCounts.total, stats.totalMatches)}</p>
        {/if}
        <nav class="botones">
          <button type="submit" class="principal" use:autofocus>{t.setupConfirm}</button>
          <button type="button" onclick={() => (setupStep = "form")}>{t.setupBack}</button>
        </nav>
      </form>
    {/if}
  {:else}
  {#if update}
    <div class="aviso" role="status">
      <span>
        {#if updating}{t.updating(update)}{:else if updateFailed}{t.updateFailed}{:else}{t.updateAvailable(update)}{/if}
      </span>
      <span class="botones">
        {#if updateFailed}
          <button class="principal" onclick={() => invoke("open_releases").catch((e) => (error = String(e)))}>
            {t.download}
          </button>
        {:else}
          <button class="principal" disabled={updating} onclick={installUpdate}>{t.update}</button>
        {/if}
        <button class="cerrar" aria-label={t.close} disabled={updating} onclick={() => (update = null)}>×</button>
      </span>
    </div>
  {/if}

  {#if showWhatsNew}
    <section class="novedades" aria-labelledby="titulo-novedades">
      <div class="fila"><span id="titulo-novedades">{t.whatsNewTitle(version)}</span></div>
      <ul>
        {#each t.whatsNew as item}<li>{item}</li>{/each}
      </ul>
      <nav class="botones">
        <button class="principal" onclick={() => save({ lastSeenVersion: version })}>{t.gotIt}</button>
      </nav>
    </section>
  {/if}

  {#if problem !== "none" && t.problems[problem]}
    <div class="problema" role="alert">
      <strong>{t.problems[problem].title}</strong>
      <span>{t.problems[problem].hint}</span>
    </div>
  {/if}

  <div class="tabs" role="tablist" aria-label={t.tabsLabel}>
    {#each visibleTabs as name (name)}
      <button
        role="tab"
        id={`tab-${name}`}
        aria-selected={tab === name}
        aria-controls={`panel-${name}`}
        tabindex={tab === name ? 0 : -1}
        class:actual={tab === name}
        onclick={() => (tab = name)}
        onkeydown={tabKey}
      >
        {t.tabs[name]}
      </button>
    {/each}
  </div>

  {#if tab === "counter" && stats}
  <div class="pestana pestana-counter" role="tabpanel" id="panel-counter" aria-labelledby="tab-counter" tabindex="-1">
    <div class="estado">
      <header class:activo={stats.inMatch && !stats.paused} class:pausa={stats.paused}>
        <i></i>{stats.paused ? t.pausedState : stats.inMatch ? t.inMatch : stats.sessionActive ? t.sessionActive : t.noSession}
      </header>
      <span class="deteccion" class:viendo={!waiting} title={t.detectionHelp}>
        {waiting ? t.detectionWaiting : t.detectionActive}
      </span>
    </div>

    {#if stats.paused}
      <div class="pausado" role="status">
        <span><strong>{t.pausedTitle}</strong>{t.pausedHint}</span>
        <button class="principal" onclick={() => run("set_paused", { paused: false })}>{t.resume}</button>
      </div>
    {/if}

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
      {#if settings && settings.dailyGoal > 0}
        {@const goal = settings.dailyGoal}
        {@const done = stats.sessionCounts.total}
        <div class="objetivo" class:cumplido={done >= goal}>
          <div
            class="barra"
            role="progressbar"
            aria-label={t.dailyGoal}
            aria-valuemin={0}
            aria-valuemax={goal}
            aria-valuenow={Math.min(done, goal)}
          >
            <span style:width="{Math.min(100, (done / goal) * 100)}%"></span>
          </div>
          <small>{done >= goal ? t.goalReached(done, goal) : t.goalProgress(done, goal)}</small>
        </div>
      {/if}
    </section>

    <section>
      <div class="fila"><span>{t.allTime}</span><strong>{stats.totalCounts.total}</strong></div>
      {@render chips(stats.totalCounts)}
      <small class="linea">{t.matches}: {stats.totalMatches}</small>
    </section>

    <nav>
      <button onclick={() => run("start_session")} disabled={stats.sessionActive}>{t.start}</button>
      <button onclick={() => run("stop_session")} disabled={!stats.sessionActive}>{t.stop}</button>
      <button onclick={() => run("new_match")} disabled={stats.paused}>{t.newMatch}</button>
      {#if !stats.paused}
        <button onclick={() => run("set_paused", { paused: true })}>{t.pause}</button>
      {/if}
      <button onclick={copySummary} aria-live="polite">{copied ? t.copied : t.copySummary}</button>
    </nav>

    {#if undoable}
      <section class="deshacer">
        {#if confirmingUndo}
          <span>{t.undoConfirm(t.kindNames[undoable.kind] ?? undoable.kind)}</span>
          <span class="botones">
            <button
              class="peligro"
              onclick={() => {
                confirmingUndo = false;
                run("undo_last_chest");
              }}
            >
              {t.undoYes}
            </button>
            <button onclick={() => (confirmingUndo = false)}>{t.cancel}</button>
          </span>
        {:else}
          <span>{t.lastCounted(t.kindNames[undoable.kind] ?? undoable.kind, Math.max(0, Math.round((now - undoable.at) / 1000)))}</span>
          <button onclick={() => (confirmingUndo = true)}>{t.undo}</button>
        {/if}
      </section>
    {/if}
  </div>
  {/if}

  {#if tab === "stats" && stats}
    {@const games = stats.totalMatches}
    {@const average = (n: number) => (games > 0 ? (n / games).toFixed(2) : "—")}
  <div class="pestana pestana-stats" role="tabpanel" id="panel-stats" aria-labelledby="tab-stats" tabindex="-1">
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
    {#if allMatches}
      <section class="dias" aria-labelledby="titulo-dias">
        <div class="fila"><span id="titulo-dias">{t.lastDaysTitle}</span></div>
        {#if lastDays.length}
          <table>
            <thead>
              <tr><th></th><th>{t.matches}</th><th>{t.chestsCol}</th><th>{t.perMatch}</th></tr>
            </thead>
            <tbody>
              {#each lastDays as day (day.key)}
                <tr>
                  <td>{day.label}</td>
                  <td>{day.matches}</td>
                  <td>{day.chests}</td>
                  <td>{(day.chests / day.matches).toFixed(1)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {:else}
          <p class="vacio">{t.lastDaysEmpty}</p>
        {/if}
      </section>
    {/if}
    {#if records}
      {@const most = records.mostChests}
      {@const longest = records.longest}
      <section class="records" aria-labelledby="titulo-records">
        <div class="fila"><span id="titulo-records">{t.recordsTitle}</span></div>
        <dl>
          <div>
            <dt>{t.recordMostChests}</dt>
            <dd>
              <b>{most ? most.counts.total : "—"}</b>
              {#if most}<small>{day(most.startedAt)} · {clock(most.durationMs)}</small>{/if}
            </dd>
          </div>
          <div>
            <dt>{t.recordLongest}</dt>
            <dd>
              <b>{longest ? clock(longest.durationMs) : "—"}</b>
              {#if longest}<small>{day(longest.startedAt)} · {t.summaryChests(longest.counts.total, true)}</small>{/if}
            </dd>
          </div>
          <div>
            <dt>{t.recordDayAverage}<Help text={t.help.recordDayAverage} /></dt>
            <dd>
              <b>{records.dayMatches > 0 ? (records.dayChests / records.dayMatches).toFixed(1) : "—"}</b>
              {#if records.dayMatches > 0}<small>{t.historySummary(records.dayMatches, records.dayChests)}</small>{/if}
            </dd>
          </div>
        </dl>
        {#if !most && !longest}<p class="vacio">{t.recordsEmpty}</p>{/if}
      </section>
    {/if}
  </div>
  {/if}

  {#if tab === "history"}
  <div class="pestana pestana-history" role="tabpanel" id="panel-history" aria-labelledby="tab-history" tabindex="-1">
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
              <span>
                <span class="duracion" title={t.duration}>{clock(match.durationMs)}</span>
                <small class="hora" title={t.startedAt}>{hourOf(match.startedAt)}</small>
              </span>
              <strong>{match.counts.total}</strong>
            </div>
            {@render chips(match.counts)}
          </li>
        {/each}
      </ul>
    {/if}
  </div>
  {/if}

  {#if tab === "settings" && settings}
  <div class="pestana pestana-settings" role="tabpanel" id="panel-settings" aria-labelledby="tab-settings" tabindex="-1">
    <section>
      <div class="opcion">
        <span>{t.language}</span>
        <select value={lang} onchange={(e) => save({ language: e.currentTarget.value })}>
          <option value="es">Español</option>
          <option value="en">English</option>
        </select>
      </div>
      <label class="opcion">
        <span>{t.startWithWindows}<Help text={t.help.startWithWindows} /></span>
        <input
          type="checkbox"
          checked={settings.startWithWindows}
          onchange={(e) => save({ startWithWindows: e.currentTarget.checked })}
        />
      </label>
      <div class="opcion">
        <label for="objetivo">{t.dailyGoal}<Help text={t.help.dailyGoal} /></label>
        <input
          id="objetivo"
          class="numero"
          type="number"
          min="0"
          max="999"
          step="1"
          inputmode="numeric"
          placeholder="0"
          value={settings.dailyGoal || ""}
          onchange={(e) => {
            const goal = Math.min(999, Math.max(0, Math.trunc(Number(e.currentTarget.value) || 0)));
            e.currentTarget.value = goal ? String(goal) : "";
            save({ dailyGoal: goal });
          }}
        />
      </div>
      <div class="opcion">
        <span>{t.theme}</span>
        <select value={settings.theme} onchange={(e) => save({ theme: e.currentTarget.value })}>
          {#each Object.entries(t.themes) as [value, name]}
            <option {value}>{name}</option>
          {/each}
        </select>
      </div>
      <div class="opcion">
        <span>{t.onClose}<Help text={t.help.onClose} /></span>
        <select
          value={settings.closeToTray ? "tray" : "exit"}
          onchange={(e) => save({ closeToTray: e.currentTarget.value === "tray" })}
        >
          <option value="tray">{t.closeTray}</option>
          <option value="exit">{t.closeExit}</option>
        </select>
      </div>
      <div class="opcion">
        <span>{t.gameArea}<Help text={t.help.gameArea} /></span>
        <b>
          {#if diag?.gameWidth}
            {diag.gameWidth} × {diag.gameHeight} · {diag.gameWindow ? t.gameWindow : t.gameScreen}
          {:else}
            —
          {/if}
        </b>
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
      <nav class="botones">
        {#if movingOverlay}
          <button class="principal" onclick={() => moveOverlay(false, true)}>{t.moveSave}</button>
          <button onclick={() => moveOverlay(false)}>{t.cancel}</button>
        {:else}
          <button onclick={() => moveOverlay(true)}>{t.moveOverlay}</button>
        {/if}
      </nav>
      {#if movingOverlay}<p class="nota-mover" role="status">{t.moveHint}</p>{/if}
      <div class="opcion">
        <span>{t.optMatch}<Help text={t.help.optMatch} /></span>
        <select value={settings.overlayMatch} onchange={(e) => save({ overlayMatch: e.currentTarget.value })}>
          <option value="off">{t.optOff}</option>
          <option value="day">{t.optDay}</option>
          <option value="total">{t.optAllTime}</option>
        </select>
      </div>
      <label class="opcion">
        <span>{t.optOnlyInMatch}<Help text={t.help.optOnlyInMatch} /></span>
        <input
          type="checkbox"
          checked={settings.overlayOnlyInMatch}
          onchange={(e) => save({ overlayOnlyInMatch: e.currentTarget.checked })}
        />
      </label>
      {#if settings.dailyGoal > 0}
        <label class="opcion">
          <span>{t.optGoal}</span>
          <input
            type="checkbox"
            checked={settings.overlayGoal}
            onchange={(e) => save({ overlayGoal: e.currentTarget.checked })}
          />
        </label>
      {/if}
      <label class="opcion">
        <span>{t.optFlash}<Help text={t.help.optFlash} /></span>
        <input
          type="checkbox"
          checked={settings.overlayFlash}
          onchange={(e) => save({ overlayFlash: e.currentTarget.checked })}
        />
      </label>
      <label class="opcion">
        <span>{t.optSummary}<Help text={t.help.optSummary} /></span>
        <input
          type="checkbox"
          checked={settings.matchSummary}
          onchange={(e) => save({ matchSummary: e.currentTarget.checked })}
        />
      </label>
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
      <div class="fila"><span>{t.shortcutsTitle}</span></div>
      {@render hotkeyRow("overlayHotkey", t.hotkey, t.help.hotkey)}
      {@render hotkeyRow("pauseHotkey", t.pauseHotkey, t.help.pauseHotkey)}
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
      <div class="opcion">
        <span>{t.backups}<Help text={t.help.backups} /></span>
        <span class="botones">
          {#if backups}
            <small>{backups.latest ? t.backupsSummary(backups.count, backupDay(backups.latest)) : t.backupsNone}</small>
          {/if}
          <button onclick={() => invoke("open_backups_dir").catch((e) => (error = String(e)))}>{t.openFolder}</button>
          <button onclick={openRestore} disabled={backupFiles !== null}>{t.restore}</button>
        </span>
      </div>
      {#if backupFiles}
        {#if backupFiles.length}
          <div class="opcion">
            <label for="copia">{t.restoreWhich}</label>
            <select id="copia" bind:value={restoreChoice}>
              {#each backupFiles as file (file.name)}
                <option value={file.name}>{backupLabel(file)}</option>
              {/each}
            </select>
          </div>
          <p class="nota-mover" role="note">{t.restoreWarning}</p>
          <nav class="botones">
            <button class="peligro" onclick={restoreBackup}>{t.restoreConfirm}</button>
            <button onclick={() => (backupFiles = null)}>{t.cancel}</button>
          </nav>
        {:else}
          <p class="nota">{t.backupsNone}</p>
        {/if}
      {/if}
      {#if restoreDone}<p class="nota" role="status">{t.restoreDone}</p>{/if}
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

    {#if version}
      <p class="version">Fortnite Chest Counter · v{version}</p>
    {/if}
  </div>
  {/if}

  {#if tab === "diag" && diag}
  <div class="pestana pestana-diag" role="tabpanel" id="panel-diag" aria-labelledby="tab-diag" tabindex="-1">
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
  </div>
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
    --warn: #fbbf24;
    --ok: #4ade80;
    /* Un color por tipo de contenedor, parecido al del juego. */
    --tesoro: #fbbf24;
    --raro: #60a5fa;
    --espiritu: #e879f9;
  }

  /* Tema claro: la misma paleta violeta sobre fondo claro, con contraste
     suficiente (texto secundario y colores de aviso más oscuros). */
  :global(:root[data-theme="light"]) {
    color-scheme: light;
    --bg: #f5f3ff;
    --card: #ffffff;
    --muted: #ede9fe;
    --border: #d8d0f5;
    --text: #1e1b4b;
    --text-muted: #57537a;
    --primary: #7c3aed;
    --primary-soft: #6d28d9;
    --accent: #e11d48;
    --danger: #dc2626;
    --warn: #b45309;
    --ok: #15803d;
    --tesoro: #d97706;
    --raro: #2563eb;
    --espiritu: #c026d3;
  }

  :global(:root) {
    color-scheme: dark;
  }

  /* Foco visible para quien usa el teclado (no con el ratón). */
  :global(:focus-visible) {
    outline: 2px solid var(--primary-soft);
    outline-offset: 2px;
  }

  @media (prefers-reduced-motion: reduce) {
    :global(*),
    :global(*::before),
    :global(*::after) {
      transition: none !important;
      animation: none !important;
    }
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

  .pestana {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-width: 0;
  }

  .pestana:focus {
    outline: none;
  }

  /* Panel estrecho: pestañas en una fila que se desplaza y números algo más
     pequeños, para que todo quepa sin cortar. */
  @media (max-width: 379px) {
    main .tabs {
      flex-wrap: nowrap;
      overflow-x: auto;
      scrollbar-width: none;
    }

    main .tabs button {
      flex: none;
      padding: 8px 7px;
    }

    main strong {
      font-size: 26px;
    }

    main section {
      padding: 10px 12px;
    }
  }

  /* Panel ancho: se aprovecha el espacio en columnas. */
  @media (min-width: 760px) {
    main {
      max-width: 980px;
    }

    .pestana-counter {
      display: grid;
      grid-template-columns: repeat(3, minmax(0, 1fr));
      align-items: start;
    }

    .pestana-counter > :global(:not(section)) {
      grid-column: 1 / -1;
    }

    .pestana-stats {
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      align-items: start;
    }

    .pestana-stats > :global(.fila) {
      grid-column: 1 / -1;
    }

    .pestana-history .historial {
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }

    .pestana-settings {
      display: block;
      columns: 2;
      column-gap: 12px;
    }

    .pestana-settings > :global(*) {
      break-inside: avoid;
      margin-bottom: 12px;
    }
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

  .estado {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 6px 12px;
  }

  .deteccion {
    padding: 2px 9px;
    border: 1px solid var(--border);
    border-radius: 999px;
    font-size: 12px;
    color: var(--text-muted);
  }

  .deteccion.viendo {
    border-color: color-mix(in srgb, var(--ok) 50%, var(--border));
    color: var(--ok);
  }

  .objetivo {
    margin-top: 8px;
  }

  .barra {
    height: 8px;
    border-radius: 999px;
    background: var(--muted);
    overflow: hidden;
  }

  .barra span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--primary);
    transition: width 300ms ease-out;
  }

  .objetivo.cumplido .barra span {
    background: var(--ok);
  }

  .objetivo small {
    display: block;
    margin-top: 4px;
    font-size: 12px;
  }

  .objetivo.cumplido small {
    color: var(--ok);
    font-weight: 600;
  }

  input.numero {
    width: 6em;
    text-align: right;
  }

  .hora {
    margin-left: 8px;
    font-size: 12px;
  }

  .nota {
    margin: 0;
    color: var(--text-muted);
    font-size: 13px;
    overflow-wrap: anywhere;
  }

  .dias table {
    margin-top: 6px;
  }

  .novedades {
    border-color: color-mix(in srgb, var(--primary) 60%, var(--border));
  }

  .novedades ul {
    margin: 8px 0;
    padding-left: 20px;
    font-size: 14px;
    line-height: 1.6;
  }

  header.pausa {
    color: var(--warn);
  }

  header.pausa i {
    border-radius: 2px;
    background: var(--warn);
  }

  /* Conteo en pausa: bien visible, con el botón para reanudar. */
  .pausado {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px 12px;
    padding: 10px 12px 10px 14px;
    border: 1px solid color-mix(in srgb, var(--warn) 55%, var(--border));
    border-left: 4px solid var(--warn);
    border-radius: 10px;
    background: color-mix(in srgb, var(--warn) 10%, var(--card));
    font-size: 13px;
    color: var(--text-muted);
  }

  .pausado strong {
    display: block;
    font-size: 14px;
    line-height: 1.3;
    color: var(--warn);
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
    color: #fff;
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
    align-items: center;
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

  .inicio input[type="number"] {
    width: 9em;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .inicio input[aria-invalid="true"] {
    border-color: var(--danger);
    outline-color: var(--danger);
  }

  .campo {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .error-campo {
    align-self: flex-end;
    color: var(--danger);
    font-size: 12px;
  }

  .resumen {
    margin: 0;
    padding: 4px 14px;
    background: var(--card);
    border: 1px solid var(--border);
    border-radius: 10px;
  }

  .resumen div {
    display: flex;
    justify-content: space-between;
    padding: 8px 0;
    border-top: 1px solid var(--border);
    font-size: 14px;
  }

  .resumen div:first-child {
    border-top: none;
  }

  .resumen dd {
    margin: 0;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }

  .resumen .suma {
    color: var(--primary-soft);
    font-weight: 700;
  }

  .borrar {
    margin: 0;
    padding: 10px 12px;
    border-left: 4px solid var(--danger);
    border-radius: 8px;
    background: color-mix(in srgb, var(--danger) 10%, var(--card));
    font-size: 13px;
    line-height: 1.45;
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

  /* Aviso de que no se ve el juego: título y pista de qué hacer. */
  .problema {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 12px 10px 14px;
    border: 1px solid color-mix(in srgb, var(--warn) 55%, var(--border));
    border-left: 4px solid var(--warn);
    border-radius: 10px;
    background: color-mix(in srgb, var(--warn) 10%, var(--card));
    font-size: 13px;
    line-height: 1.45;
    color: var(--text-muted);
  }

  .problema strong {
    font-size: 14px;
    line-height: 1.3;
    letter-spacing: 0;
    color: var(--warn);
  }

  button.principal {
    background: var(--primary);
    border-color: var(--primary);
    color: #fff;
    font-weight: 600;
  }

  button.cerrar {
    min-width: 36px;
    padding: 0 10px;
    font-size: 18px;
    line-height: 1;
  }

  .deshacer {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px 12px;
    font-size: 14px;
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

  .atajo {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--text-muted);
  }

  .atajo em {
    font-style: normal;
    font-size: 13px;
  }

  kbd {
    min-width: 24px;
    padding: 2px 7px;
    border: 1px solid var(--border);
    border-bottom-width: 2px;
    border-radius: 6px;
    background: var(--muted);
    color: var(--text);
    font-family: inherit;
    font-size: 12px;
    font-weight: 600;
    line-height: 1.4;
    text-align: center;
  }

  .nota-mover {
    margin: 4px 0 6px;
    color: var(--warn);
    font-size: 13px;
  }

  .aviso-atajo {
    margin: 4px 0 6px;
    color: var(--danger);
    font-size: 13px;
  }

  .records {
    margin-top: 12px;
  }

  .records dl {
    margin: 8px 0 0;
  }

  .records dl > div {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 0;
    border-top: 1px solid var(--border);
  }

  .records dl > div:first-child {
    border-top: none;
  }

  .records dt {
    display: inline-flex;
    align-items: center;
    font-size: 14px;
    color: var(--text-muted);
  }

  .records dd {
    margin: 0;
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    text-align: right;
  }

  .records dd b {
    font-size: 18px;
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }

  .records dd small {
    font-size: 12px;
  }

  /* Versión instalada, discreta al pie de Configuración. */
  .version {
    margin: 4px 0 0;
    color: var(--text-muted);
    font-size: 12px;
    letter-spacing: 0.06em;
    text-align: center;
    opacity: 0.75;
    user-select: text;
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
