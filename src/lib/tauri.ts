// Llamadas al backend. Fuera de Tauri (al abrir la interfaz en un navegador
// durante el desarrollo) devuelve datos de ejemplo para poder ver el diseño.
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen, type EventCallback, type UnlistenFn } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWindow } from "@tauri-apps/api/window";

const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const counts = (normal: number, rare: number, spirit: number) => ({
  normal,
  rare,
  spirit,
  total: normal + rare + spirit,
});

const sample: Record<string, unknown> = {
  get_stats: {
    inMatch: true,
    sessionActive: true,
    sessionStartedAt: Date.now() - 4_530_000,
    matchCounts: counts(9, 2, 1),
    sessionCounts: counts(58, 7, 4),
    totalCounts: counts(4821, 366, 87),
    sessionMatches: 6,
    totalMatches: 412,
    paused: new URLSearchParams(location.search).get("pausa") !== null,
    matchStartedAt: Date.now() - 754_000,
  },
  get_settings: {
    language: "es",
    binding: { device: "key", code: 69, label: "E" },
    requireInput: false,
    hideFromCapture: true,
    showOverlay: true,
    saveSamples: false,
    diagnostics: new URLSearchParams(location.search).get("diagnostico") !== null,
    closeToTray: true,
    startWithWindows: false,
    matchSummary: true,
    overlayX: 0,
    overlayY: 0,
    overlayHotkey: "Ctrl+Shift+KeyO",
    pauseHotkey: "",
    endMatchHotkey: "",
    overlayOnlyInMatch: false,
    overlayFlash: true,
    dailyGoal: 80,
    overlayGoal: true,
    overlayTimer: true,
    theme: new URLSearchParams(location.search).get("tema") ?? "dark",
    lastSeenVersion: new URLSearchParams(location.search).get("novedades") !== null ? "1.4.0" : "1.4.1",
    corner: "top-left",
    setupDone: new URLSearchParams(location.search).get("inicio") === null,
    overlayMatch: "total",
    overlayTotal: true,
    overlayChests: "types",
    overlayColor: true,
    overlaySize: "medium",
    overlayOpacity: 100,
  },
  get_diag: {
    enabled: new URLSearchParams(location.search).get("diagnostico") !== null,
    supported: true,
    dir: "C:\\Users\\jugador\\Pictures\\FortniteChestCounter",
    inputDown: false,
    inputPresses: 31,
    pollHits: 2,
    hotkeyHits: 2,
    hotkeyError: null,
    pendingIn: null,
    recording: false,
    frames: 0,
    lastResult: null,
    detected: 12,
    detectedWithInput: 12,
    lastDetected: "rare",
    detectError: null,
    hudVisible: true,
    gameWidth: 1920,
    gameHeight: 1080,
    gameWindow: true,
  },
  is_debug: false,
  check_update: "1.2.0",
  get_records: {
    mostChests: { startedAt: Date.now() - 3 * 86_400_000, durationMs: 1_385_000, counts: counts(14, 3, 0) },
    longest: { startedAt: Date.now() - 5 * 86_400_000, durationMs: 1_702_000, counts: counts(6, 1, 1) },
    dayMatches: 4,
    dayChests: 35,
  },
  get_vision_problem: "none",
  get_detection_waiting: false,
  get_streak: { current: 3, best: 5, todayDone: false },
  list_backups: [
    { name: "estadisticas-2026-11-18.db", kind: "daily", day: "2026-11-18" },
    { name: "estadisticas-2026-11-17.db", kind: "daily", day: "2026-11-17" },
    { name: "antes-de-reiniciar.db", kind: "reset", day: null },
  ],
  get_all_matches: [
    { id: 6, startedAt: Date.now() - 1_300_000, durationMs: 1_114_000, counts: counts(9, 2, 1) },
    { id: 5, startedAt: Date.now() - 2_400_000, durationMs: 742_000, counts: counts(5, 0, 1) },
    { id: 2, startedAt: Date.now() - 86_400_000, durationMs: 900_000, counts: counts(7, 1, 0) },
    { id: 1, startedAt: Date.now() - 3 * 86_400_000, durationMs: 1_200_000, counts: counts(11, 2, 2) },
  ],
  backup_info: { count: 7, latest: "2026-11-18" },
  get_history: [
    { id: 6, startedAt: Date.now() - 1_300_000, durationMs: 1_114_000, counts: counts(9, 2, 1) },
    { id: 5, startedAt: Date.now() - 2_400_000, durationMs: 742_000, counts: counts(5, 0, 1) },
    { id: 4, startedAt: Date.now() - 3_600_000, durationMs: 96_000, counts: counts(0, 0, 0) },
    { id: 3, startedAt: Date.now() - 6_000_000, durationMs: 1_385_000, counts: counts(14, 3, 0) },
  ],
};

export function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (inTauri) return tauriInvoke<T>(command, args);
  if (command === "install_update") return Promise.reject("Solo disponible en la app de Windows.");
  return Promise.resolve((sample[command] ?? sample.get_stats) as T);
}

export function listen<T>(event: string, handler: EventCallback<T>): Promise<UnlistenFn> {
  if (inTauri) return tauriListen<T>(event, handler);
  // En el navegador, para probar: window.__emitir("evento", datos).
  const handlers = (mockHandlers[event] ??= new Set());
  const wrapped = (payload: unknown) => handler({ event, id: 0, payload: payload as T });
  handlers.add(wrapped);
  return Promise.resolve(() => handlers.delete(wrapped));
}

const mockHandlers: Record<string, Set<(payload: unknown) => void>> = {};
if (!inTauri && import.meta.env.DEV && typeof window !== "undefined") {
  (window as unknown as Record<string, unknown>).__emitir = (event: string, payload: unknown) =>
    mockHandlers[event]?.forEach((handler) => handler(payload));
}

/** Empieza a arrastrar la ventana actual con el ratón (solo en la app). */
export function startDragging(): void {
  if (inTauri) getCurrentWindow().startDragging().catch(() => {});
}

/** Versión instalada de la app (en el navegador, una de ejemplo). */
export function appVersion(): Promise<string> {
  return inTauri ? getVersion() : Promise.resolve("1.4.1");
}
