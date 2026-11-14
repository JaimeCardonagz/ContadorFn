// Aviso de versión nueva: consulta la última versión publicada en GitHub.

const LATEST = "https://api.github.com/repos/JaimeCardonagz/ContadorFn/releases/latest";

/** "v1.2.0" o "1.2.0" → [1, 2, 0]. */
function parts(version: string): number[] {
  return version
    .replace(/^v/i, "")
    .split(/[.+-]/)
    .slice(0, 3)
    .map((n) => Number.parseInt(n, 10) || 0);
}

/** ¿Es `latest` una versión posterior a `current`? */
export function isNewer(latest: string, current: string): boolean {
  const [a, b] = [parts(latest), parts(current)];
  for (let i = 0; i < 3; i++) {
    if ((a[i] ?? 0) !== (b[i] ?? 0)) return (a[i] ?? 0) > (b[i] ?? 0);
  }
  return false;
}

/** Última versión publicada (p. ej. "1.2.0"), o null si no se pudo saber. */
export async function latestVersion(): Promise<string | null> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 8000);
  try {
    const response = await fetch(LATEST, {
      headers: { Accept: "application/vnd.github+json" },
      signal: controller.signal,
    });
    if (!response.ok) return null;
    const release = (await response.json()) as { tag_name?: string; draft?: boolean; prerelease?: boolean };
    if (!release.tag_name || release.draft || release.prerelease) return null;
    return release.tag_name.replace(/^v/i, "");
  } catch {
    return null;
  } finally {
    clearTimeout(timer);
  }
}
