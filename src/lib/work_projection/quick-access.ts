export type QuickTone = "violet" | "blue" | "teal" | "green" | "amber" | "brand";

export type QuickAccessItem = {
  id: string;
  title: string;
  description?: string;
  href: string;
  icon?: string;
  tone: QuickTone;
  meta?: string;
  secondary?: { label: string; href: string };
};

export type QuickAccessPreference = {
  pinnedIds: string[] | null;
  version: number;
};

export function visibleQuickAccessItems<T extends { id: string }>(
  items: T[],
  preference: QuickAccessPreference,
  defaultIds: string[],
): T[] {
  const byId = new Map(items.map((item) => [item.id, item]));
  return (preference.pinnedIds ?? defaultIds)
    .map((id) => byId.get(id))
    .filter((item): item is T => !!item);
}
