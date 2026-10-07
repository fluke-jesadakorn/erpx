import {compareWork,isMyAction,type WorkItem} from "@/lib/work-item";
import {sourceRecordLabel} from "@/lib/source-record";
export type WorkSnapshot = { items: WorkItem[]; unavailableSources: string[] };

export function mergeWorkItems(rows: WorkItem[]) {
  const merged = new Map<string, WorkItem>();
  for (const item of rows) {
    const key = `${item.type}:${item.id}:${item.actionKind === "override" ? "override" : "record"}`,
      old = merged.get(key);
    if (!old) {
      merged.set(key, { ...item, nextAction: item.nextAction ?? item.detail });
      continue;
    }
    const actionable =
      old.queue === "todo"
        ? old
        : item.queue === "todo"
          ? item
          : old.updatedAt >= item.updatedAt
            ? old
            : item;
    merged.set(key, {
      ...actionable,
      nextAction: actionable.nextAction ?? actionable.detail,
      relationships: [
        ...new Set([
          ...(old.relationships ?? []),
          ...(item.relationships ?? []),
        ]),
      ],
    });
  }
  const items = [...merged.values()];
  // Alerts remain separate in notification history, but share one work context.
  const holds = new Map(items.filter(item => item.type === "quality_hold").map(item => [item.id, item]));
  for (const alert of items.filter(item => item.type === "quality_alert")) {
    const hold = holds.get(alert.source?.id ?? alert.href.split("/").at(-1) ?? "");
    if (!hold || hold.queue === "completed") continue;
    hold.relatedAlerts = [...(hold.relatedAlerts ?? []), { id: alert.id, title: alert.title, nextAction: alert.nextAction ?? alert.detail, completed: alert.queue === "completed" }];
    hold.relationships = [...new Set([...(hold.relationships ?? []), ...(alert.relationships ?? [])])];
    if (isMyAction(alert) && !isMyAction(hold)) {
      hold.canAct = true; hold.queue = "todo"; hold.actionKind = "work";
      hold.nextAction = alert.nextAction ?? alert.detail;
    }
  }
  return items.filter(item => item.type !== "quality_alert" || !holds.has(item.source?.id ?? item.href.split("/").at(-1) ?? "") || holds.get(item.source?.id ?? item.href.split("/").at(-1) ?? "")?.queue === "completed")
    .map(item => ({ ...item, source: item.source ?? { kind: item.type, id: item.id, href: item.href, label: sourceRecordLabel(item.type), ...(item.flow ? {version: item.flow.recordVersion} : {}) } }))
    .sort(compareWork);
}
export function nextWorkSteps(items: WorkItem[], now:number) {
  return items
    .filter(isMyAction)
    .sort((a, b) => {
      const overdueA = a.dueAt && a.dueAt < now ? 1 : 0,
        overdueB = b.dueAt && b.dueAt < now ? 1 : 0;
      return (
        overdueB - overdueA ||
        (a.dueAt ?? Infinity) - (b.dueAt ?? Infinity) ||
        b.updatedAt - a.updatedAt ||
        a.id.localeCompare(b.id)
      );
    })
    .slice(0, 3);
}
