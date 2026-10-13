import { describe, expect, it } from "vitest";
import { workAttention } from "../../../src/lib/work_projection/work-attention";
import { notificationDays } from "../../../src/lib/shared/notification-days";
import type { WorkItem } from "../../../src/lib/work_projection/work-item";

const item = (id: string, changes: Partial<WorkItem> = {}): WorkItem => ({
  id, type: "expense", title: id, detail: "Review", href: `/expenses/${id}`,
  status: "pending", queue: "todo", updatedAt: 10, ...changes,
});

describe("workspace attention", () => {
  it("keeps blocked team work out of personal attention", () => {
    const blocked = item("blocked", { queue: "waiting", relationships:["team"], blocker: "Choose an owner" });
    const result = workAttention([blocked, blocked, item("a"), item("b"), item("c"), item("waiting", { queue: "waiting", relationships:["requested"] }), item("done", { queue: "completed", blocker: "Old blocker" })], false, 100);
    expect(result.total).toBe(3);
    expect(result.items).toHaveLength(3);
    expect(result.items[0].id).toBe("a");
    expect(result.waiting).toBe(1);
  });
  it("orders overdue work, then due date, latest update and a stable identity", () => {
    const rows = [item("latest", { updatedAt: 60 }), item("due", { dueAt: 200 }), item("old"), item("overdue", { dueAt: 80 }), item("blocked", { blocker: "Missing details", dueAt: 300 })];
    expect(workAttention(rows, false, 100).items.map(i => i.id)).toEqual(["overdue", "due", "blocked"]);
    expect(workAttention([item("b"), item("a"), rows[0]], false, 100).items.map(i => i.id)).toEqual(["latest", "a", "b"]);
    expect(rows[0].id).toBe("latest");
  });
  it("uses the latest duplicate and does not collapse different work types", () => {
    expect(workAttention([item("same"), item("same", { queue: "completed", updatedAt: 20 }), item("same", { type: "staff_task" })]).total).toBe(1);
  });
  it("retains incomplete state for an empty or partially loaded snapshot", () => {
    expect(workAttention([], true)).toEqual({ items: [], total: 0, waiting: 0, incomplete: true });
    expect(workAttention([item("a")], true).incomplete).toBe(true);
    expect(workAttention([]).incomplete).toBe(false);
  });
  it("treats blank blockers as absent and accepts a due timestamp of zero", () => {
    expect(workAttention([item("blank", { queue: "waiting", blocker: " " })]).total).toBe(0);
    expect(workAttention([item("later"), item("epoch", { dueAt: 0 })], false, 100).items[0].id).toBe("epoch");
  });
});

it("groups notifications by Bangkok day across a UTC midnight boundary without mutating source order", () => {
  const rows = [{ id: "old", created_at: Date.parse("2026-09-12T16:59:00Z") }, { id: "new", created_at: Date.parse("2026-09-12T17:01:00Z") }, { id: "same-day", created_at: Date.parse("2026-09-13T01:00:00Z") }];
  const groups = notificationDays(rows);
  expect(groups.map(g => g.label)).toEqual(["13 September 2026", "12 September 2026"]);
  expect(groups[0].items.map(i => i.id)).toEqual(["same-day", "new"]);
  expect(rows[0].id).toBe("old");
});
