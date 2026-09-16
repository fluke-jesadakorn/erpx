vi.mock("../src/server/flow-work", () => ({enrichFlowWork:async (_actor:unknown,items:WorkItem[])=>items,otherFlowWork:async()=>[]}));
import { expect, it, vi } from "vitest";
import {
  mergeWorkItems,
  nextWorkSteps,
  workSnapshot,
} from "../src/server/work-items";
import type { WorkContext } from "../src/server/authorization";
import type { WorkItem } from "../src/lib/work-item";
import { filterWork } from "../src/lib/work-item";
const item = (id: string, extra: Partial<WorkItem> = {}): WorkItem => ({
  id,
  type: "staff_task",
  title: id,
  detail: "Review this item",
  href: `/staff-tasks/${id}`,
  status: "queued",
  updatedAt: 1,
  queue: "todo",
  ...extra,
});
vi.mock("../src/server/runtime", () => ({
  holdingResources: async () => ({ write: {} }),
}));
vi.mock("../src/server/domain-work", () => ({
  getDomainWork: async (_actor: unknown, source: string) => {
    if (source === "expense") throw Error("Storage temporarily unavailable");
    return source === "staff_task" ? [item("task")] : [];
  },
}));
vi.mock("../src/server/work", () => ({ getWork: async () => ({ rows: [] }) }));
vi.mock("../src/server/employee-requests", () => ({
  employeeRequestWork: async () => [
    item("request", { type: "service_request", relationships: ["requested"] }),
  ],
}));
it("deduplicates a record found through several responsibilities and keeps all viewer relationships", () => {
  const rows = mergeWorkItems([
    item("same", { queue: "waiting", relationships: ["requested"] }),
    item("same", { queue: "todo", relationships: ["team", "assigned"] }),
  ]);
  expect(rows).toHaveLength(1);
  expect(rows[0].queue).toBe("todo");
  expect(rows[0].relationships).toEqual(["requested", "team", "assigned"]);
  expect(filterWork(rows, "todo", "all", "", "assigned")).toHaveLength(1);
  expect(filterWork(rows, "todo", "all", "", "approval")).toHaveLength(0);
});
it("limits Home priorities to three actionable items from any source, with overdue work first", () => {
  const rows = [
    item("new", { updatedAt: 100 }),
    item("expense", { type: "expense", dueAt: 200 }),
    item("overdue", { type: "equipment_request", dueAt: 1 }),
    item("waiting", { queue: "waiting", dueAt: 1 }),
    item("done", { queue: "completed" }),
    item("late", { dueAt: 500 }),
  ];
  expect(nextWorkSteps(rows, 100).map((i) => i.id)).toEqual([
    "overdue",
    "expense",
    "late",
  ]);
});
it("retains the workspace when one source fails and reports only that source", async () => {
  const actor = {
    companyId: "company",
    holdingId: "h001",
    role: "employee",
    assignments: [],
    scope: { kind: "holding", companyId: "company" },
  } as WorkContext;
  const error = vi.spyOn(console, "error").mockImplementation(() => {});
  try {
    const snapshot = await workSnapshot(actor);
    expect(snapshot.items.map((i) => i.id).sort()).toEqual(["request", "task"]);
    expect(snapshot.unavailableSources).toEqual(["Expense claims"]);
  } finally {
    error.mockRestore();
  }
});
