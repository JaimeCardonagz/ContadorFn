// Llamadas al backend. Fuera de Tauri (al abrir la interfaz en un navegador
// durante el desarrollo) devuelve datos de ejemplo para poder ver el diseño.
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen, type EventCallback, type UnlistenFn } from "@tauri-apps/api/event";

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
  },
  get_settings: {
    language: "es",
    binding: { device: "key", code: 69, label: "E" },
    requireInput: false,
    hideFromCapture: true,
    showOverlay: true,
    corner: "top-left",
    setupDone: new URLSearchParams(location.search).get("inicio") === null,
    overlayMatch: "total",
    overlayTotal: true,
    overlayChests: "types",
  },
  get_diag: {
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
  },
  is_debug: false,
};

export function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (inTauri) return tauriInvoke<T>(command, args);
  return Promise.resolve((sample[command] ?? sample.get_stats) as T);
}

export function listen<T>(event: string, handler: EventCallback<T>): Promise<UnlistenFn> {
  if (inTauri) return tauriListen<T>(event, handler);
  return Promise.resolve(() => {});
}
