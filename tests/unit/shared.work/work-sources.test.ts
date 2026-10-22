import {createWorkSnapshot} from "@erpx/server-work_projection/application/work-items";
import { expect, it, vi } from "vitest";
import {
  mergeWorkItems,
  nextWorkSteps,
} from "@erpx/server-work_projection/domain/work-items";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { WorkItem } from "@erpx/feature-work_projection/lib/work-item";
import { filterWork } from "@erpx/feature-work_projection/lib/work-item";
import { operationFromWork } from "@erpx/feature-shared/lib/operation-presentation";
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
it("keeps a department override separate from normal approval work for the same record", () => {
  const rows=mergeWorkItems([
    item("same",{type:"expense",href:"/expenses/same",relationships:["approval"]}),
    item("same",{type:"expense",href:"/work/department-approvals/expense/same",actionKind:"override",relationships:["approval"]}),
  ]);
  expect(rows).toHaveLength(2);
  expect(rows.map(row=>operationFromWork(row).actionKind).sort()).toEqual(["approval","override"]);
  expect(rows.find(row=>row.actionKind==="override")?.href).toBe("/work/department-approvals/expense/same");
  expect(rows.every(row=>operationFromWork(row).dueAt===null)).toBe(true);
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

it("groups active quality follow-ups with their source hold and retains notification history references",()=>{
  const source={kind:"quality_hold",id:"hold",href:"/quality/hold",label:"Quality hold",version:3};
  const rows=mergeWorkItems([
    item("hold",{type:"quality_hold",source,href:source.href,actionKind:"approval",relationships:["approval"]}),
    item("alert",{type:"quality_alert",source,href:source.href,actionKind:"work",relationships:["assigned"]}),
  ]);
  expect(rows).toHaveLength(1);expect(rows[0].source).toEqual(source);
  expect(rows[0].actionKind).toBe("approval");
  expect(rows[0].relatedAlerts?.map(alert=>alert.id)).toEqual(["alert"]);
  expect(rows[0].relationships).toEqual(["approval","assigned"]);
  expect(mergeWorkItems([item("hold",{type:"quality_hold",queue:"completed"}),item("alert",{type:"quality_alert",source})])).toHaveLength(2);
});
it("retains the workspace when one source fails and reports only that source", async () => {
  const actor: WorkContext = {
    userId: "user",
    name: "Employee",
    email: "employee@example.test",
    assignmentId: "assignment",
    companyId: "company",
    holdingId: "h001",
    holdingSlug: "acme",
    holdingName: "Acme",
    branchId: null,
    departmentId: null,
    role: "employee",
    modules: [],
    assignments: [],
    workspaces: [],
    assignmentScope: "holding",
    inherited: false,
    scope: { kind: "holding", holdingId: "h001", holdingSlug: "acme", holdingName: "Acme", companyId: "company" },
  };
  const warning = vi.spyOn(console, "warn").mockImplementation(() => {});
  try {
    const app=createWorkSnapshot({source:async name=>name==="employeeRequestWork"?[item("request",{type:"service_request",relationships:["requested"]})]:[],domain:async(_actor,source)=>{if(source==="expense")throw Error("Storage temporarily unavailable");return source==="staff_task"?[item("task")]:[];},leave:async()=>({rows:[],counts:{todo:0,waiting:0,completed:0,all:0}}),enrich:async(_actor,items)=>items,enabledModules:async()=>new Set(),production:()=>false,monotonic:()=>0,performanceLog:()=>false,errorStatus:()=>undefined,info:()=>{},warn:(...values)=>console.warn(...values),cached:(_actor,load)=>load()});
    const snapshot = await app.readWorkSnapshot(actor);
    expect(snapshot.items.map((i) => i.id).sort()).toEqual(["request", "task"]);
    expect(snapshot.unavailableSources).toEqual(["Expense claims"]);
  } finally {
    warning.mockRestore();
  }
});
