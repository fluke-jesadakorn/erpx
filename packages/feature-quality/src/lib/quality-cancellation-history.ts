import type { NativeQualityCancellationPage } from '../adapters/server/native-quality';

/** The caller supplies a trusted selected-context stamp, not an HTTP input. */
export function qualityHistorySessionKey(holdId: string, contextResetKey: string): string {
  return JSON.stringify([holdId, contextResetKey]);
}

export function beginQualityHistoryRequest(slot: { current: AbortController | null }, controller: AbortController): boolean {
  if (slot.current) return false;
  slot.current = controller;
  return true;
}

export function finishQualityHistoryRequest(slot: { current: AbortController | null }, controller: AbortController): void {
  if (slot.current === controller) slot.current = null;
}

/** Never accept an empty continuation or merge different snapshots. */
export function mergeQualityHistory(
  current: NativeQualityCancellationPage,
  next: NativeQualityCancellationPage,
): NativeQualityCancellationPage | null {
  if (current.nextBefore === null || next.items.length === 0 ||
      next.headVersion !== current.headVersion || next.historyRevision !== current.historyRevision ||
      next.items.some(item => item.version >= current.nextBefore!) ||
      next.items.some(item => current.items.some(previous => previous.id === item.id))) return null;
  return { ...next, items: [...current.items, ...next.items] };
}
