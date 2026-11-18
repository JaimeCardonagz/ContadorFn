/** Duración legible: "23 min 05 s", o "1 h 02 min" si pasa de una hora. */
export function clock(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000));
  const pad = (n: number) => String(n).padStart(2, "0");
  const h = Math.floor(s / 3600);
  const m = Math.floor(s / 60) % 60;
  if (h > 0) return `${h} h ${pad(m)} min`;
  return m > 0 ? `${m} min ${pad(s % 60)} s` : `${s} s`;
}
