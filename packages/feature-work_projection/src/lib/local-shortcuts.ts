import type { QuickAccessPreference } from "@erpx/feature-work_projection/lib/quick-access";

export function localShortcutKey(userId: string, companyId: string, base: string) {
  return `erpx:shortcuts:v1:${JSON.stringify([userId, companyId, base])}`;
}

export function parseLocalShortcuts(raw: string | null): QuickAccessPreference | null {
  if (!raw) return null;
  try {
    const value = JSON.parse(raw);
    if (value.schema !== 1 || !Number.isSafeInteger(value.version) || value.version < 0) return null;
    const ids: unknown = value.pinnedIds;
    if (ids !== null && (!Array.isArray(ids) || ids.length > 6 ||
      ids.some(id => typeof id !== "string" || !id || id.length > 100) ||
      new Set(ids).size !== ids.length)) return null;
    return { pinnedIds: ids as string[] | null, version: value.version };
  } catch {
    return null;
  }
}

export function serializeLocalShortcuts(preference: QuickAccessPreference) {
  return JSON.stringify({ schema: 1, ...preference });
}
