import { isMyAction, compareWork, workInView, type WorkItem } from "@erpx/feature-work_projection/lib/work-item";

export type WorkAttention = {
  items: WorkItem[];
  total: number;
  waiting: number;
  incomplete: boolean;
};

export function workBlocker(item: WorkItem) {
  return item.flow?.blocker?.trim() || item.blocker?.trim() || null;
}

/** Presentation only: queue membership and authorization belong to the source. */
export function workAttention(
  items: WorkItem[],
  incomplete = false,
  ..._clock: [now?: number]
): WorkAttention {
  void _clock; // Retain the public clock argument for callers and tests.
  const unique = new Map<string, WorkItem>();
  for (const item of items) {
    const key = `${item.type}:${item.id}`;
    const previous = unique.get(key);
    if (!previous || item.updatedAt > previous.updatedAt) unique.set(key, item);
  }
  const rows = [...unique.values()];
  const attention = rows
    .filter(isMyAction)
    .sort(compareWork);
  return {
    items: attention.slice(0, 3),
    total: attention.length,
    waiting: workInView(rows, "waiting").length,
    incomplete,
  };
}
