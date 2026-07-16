export type ThemeMode = 'light' | 'dark' | 'system';
export type ResolvedTheme = 'light' | 'dark';

export const THEME_STORAGE_KEY = 'folio.theme';

export function getStoredTheme(): ThemeMode {
  if (typeof window === 'undefined') return 'dark';
  const v = window.localStorage.getItem(THEME_STORAGE_KEY);
  if (v === 'light' || v === 'dark' || v === 'system') return v;
  return 'dark';
}

export function setStoredTheme(mode: ThemeMode) {
  if (typeof window === 'undefined') return;
  try { window.localStorage.setItem(THEME_STORAGE_KEY, mode); } catch {}
}

export function resolveTheme(mode: ThemeMode): ResolvedTheme {
  if (mode === 'light' || mode === 'dark') return mode;
  if (typeof window === 'undefined') return 'dark';
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'dark';
}
