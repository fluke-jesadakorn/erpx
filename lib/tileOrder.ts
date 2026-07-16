export type TileOrderMap = Record<string, string[]>;

const key = (userId: number) => `werp:tile-order:${userId}`;

export function loadOrder(userId: number): TileOrderMap {
  if (typeof window === 'undefined') return {};
  try {
    return JSON.parse(localStorage.getItem(key(userId)) || '{}') as TileOrderMap;
  } catch {
    return {};
  }
}

export function saveOrder(userId: number, map: TileOrderMap) {
  if (typeof window === 'undefined') return;
  localStorage.setItem(key(userId), JSON.stringify(map));
}

export function saveGroupOrder(userId: number, group: string, ids: string[]) {
  const map = loadOrder(userId);
  map[group] = ids;
  saveOrder(userId, map);
}

export function hasGroupOrder(map: TileOrderMap, group: string): boolean {
  const ids = map[group];
  return Array.isArray(ids) && ids.length > 0;
}

export function clearGroupOrder(userId: number, group: string) {
  if (typeof window === 'undefined') return;
  const map = loadOrder(userId);
  delete map[group];
  saveOrder(userId, map);
}

export function clearOrder(userId: number) {
  if (typeof window === 'undefined') return;
  localStorage.removeItem(key(userId));
}

function bucket(state: string | undefined): 0 | 1 {
  return state === 'open' ? 0 : 1;
}

export function applyOrder<
  T extends { tile: { id: string; group: string }; access?: { state?: string } },
>(
  items: T[],
  group: string,
  order: TileOrderMap,
): T[] {
  const ids = order[group];
  const indexOf = (id: string) => {
    if (!ids || ids.length === 0) return Number.MAX_SAFE_INTEGER;
    const i = ids.indexOf(id);
    return i === -1 ? Number.MAX_SAFE_INTEGER : i;
  };
  return [...items].sort((a, b) => {
    const ba = bucket(a.access?.state);
    const bb = bucket(b.access?.state);
    if (ba !== bb) return ba - bb;
    return indexOf(a.tile.id) - indexOf(b.tile.id);
  });
}