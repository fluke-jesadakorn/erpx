const dayFormat = new Intl.DateTimeFormat("en-CA", { timeZone: "Asia/Bangkok", year: "numeric", month: "2-digit", day: "2-digit" });
const labelFormat = new Intl.DateTimeFormat("en-GB", { timeZone: "Asia/Bangkok", day: "numeric", month: "long", year: "numeric" });

export function notificationDays<T extends { created_at: number }>(items: T[]) {
  const groups = new Map<string, { key: string; label: string; items: T[] }>();
  for (const item of [...items].sort((a, b) => b.created_at - a.created_at)) {
    const key = dayFormat.format(item.created_at);
    const group = groups.get(key) ?? { key, label: labelFormat.format(item.created_at), items: [] };
    group.items.push(item);
    groups.set(key, group);
  }
  return [...groups.values()];
}
