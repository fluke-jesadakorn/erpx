import { expect, it } from "vitest";
import { filterWork, workInView, workViews, type WorkItem } from "../src/lib/work-item";
import { projectWorkPage, type WorkPageQuery } from "../src/lib/work-page";
import type { WorkSummary } from "../src/server/work-summary";

const query: WorkPageQuery = { view: "actions", queue: "todo", type: "all", q: "", responsibility: "all" };
const item = (index: number, extra: Partial<WorkItem> = {}): WorkItem => ({
  id: String(index), type: "staff_task", title: `Task ${index}`, detail: "Review work",
  href: `/staff-tasks/${index}`, status: "ready", updatedAt: index, queue: "todo", ...extra,
});
const summary = (items: WorkItem[], unavailableSources: string[] = []): WorkSummary => ({
  items, unavailableSources, generatedAt: 1, nextSteps: items.slice(0, 3), cards: [],
  capabilities: { creation: [], teamQueue: false, unavailable: [] },
  preferences: { pinnedIds: null, version: 0 }, preferencesUnavailable: false,
});

it("keeps all-source totals and actions independent of a 30-row page", () => {
  const snapshot = summary([
    ...Array.from({ length: 65 }, (_, index) => item(index, { relationships: ["assigned"] })),
    item(100, { type: "expense", queue: "waiting", relationships: ["requested", "approval"] }),
    item(101, { type: "pms", queue: "completed", relationships: ["team"] }),
  ]);
  const first = projectWorkPage(snapshot, query), second = projectWorkPage(snapshot, { ...query, offset: 30 });
  expect(first.items).toHaveLength(30);
  expect(second.items).toHaveLength(30);
  expect(first.list).toEqual({ offset: 0, total: 65, hasMore: true });
  expect(second.list).toEqual({ offset: 30, total: 65, hasMore: true });
  expect(first.actionCount).toBe(65);
  expect(second.actionCount).toBe(65);
  expect(first.viewCounts).toMatchObject({ actions: 65, waiting: 1, completed: 1, all: 67 });
  expect(first.relationships).toEqual(["assigned", "requested", "approval", "team"]);
  expect(snapshot.items).toHaveLength(67);
});

it("matches the existing filters and legacy queue counts for every view", () => {
  const snapshot = summary([
    item(1, { type: "expense", title: "Hotel expense", relationships: ["requested"] }),
    item(2, { type: "expense", title: "Hotel expense", queue: "waiting", relationships: ["requested", "approval"] }),
    item(3, { type: "expense", title: "Hotel expense", queue: "completed", relationships: ["requested"] }),
    item(4, { type: "expense", title: "Hotel team", canAct: false, category: "setup", relationships: ["team"] }),
    item(5, { title: "Hotel task", relationships: ["requested"] }),
    item(6, { type: "expense", title: "Other expense", relationships: ["requested"] }),
  ]);
  for (const view of [...workViews.map(([id]) => id), "legacy"]) {
    const filters = { ...query, view, queue: "waiting", type: "expense", q: "HOTEL", responsibility: "requested" };
    const page = projectWorkPage(snapshot, filters);
    const expected = filterWork(workInView(snapshot.items, view), view === "legacy" ? "waiting" : "all", "expense", "HOTEL", "requested");
    expect(page.items).toEqual(expected);
    expect(page.list.total).toBe(expected.length);
    for (const [id] of workViews)
      expect(page.viewCounts[id]).toBe(filterWork(workInView(snapshot.items, id), "all", "expense", "HOTEL", "requested").length);
    expect(page.actionCount).toBe(3);
  }
});

it("clamps missing pages, rejects invalid offsets and preserves incomplete action counts", () => {
  const snapshot = summary(Array.from({ length: 65 }, (_, index) => item(index)), ["Approvals"]);
  expect(projectWorkPage(snapshot, { ...query, offset: 90 }).list).toEqual({ offset: 60, total: 65, hasMore: false });
  for (const offset of [-1, NaN, Infinity, 1.5])
    expect(projectWorkPage(snapshot, { ...query, offset }).list.offset).toBe(0);
  expect(projectWorkPage(snapshot, query).actionCount).toBeNull();
  expect(projectWorkPage(summary([]), query).list).toEqual({ offset: 0, total: 0, hasMore: false });
});
