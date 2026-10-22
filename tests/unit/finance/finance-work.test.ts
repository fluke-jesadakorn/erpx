import { describe, expect, it } from "vitest";
import { financeWork } from "@erpx/feature-finance/lib/finance-work";
import type { WorkItem } from "@erpx/feature-work_projection/lib/work-item";

const row = (id: string, overrides: Partial<WorkItem> = {}): WorkItem => ({ id, type: "invoice", title: id, detail: "Review source evidence", href: `/finance/invoices/${id}`, status: "draft", updatedAt: 1, queue: "todo", canAct: true, relationships: ["assigned"], ...overrides });

describe("Finance responsibilities", () => {
  it("uses source authority rather than counting every Company draft as my work", () => {
    const items = [row("mine"), row("other", { canAct: false, queue: "waiting", relationships: ["team"] }), row("complete", { queue: "completed" }), row("outside", { type: "staff_task" })];
    expect(financeWork(items, "actions").map(item => item.id)).toEqual(["mine"]);
    expect(financeWork(items, "waiting")).toEqual([]);
    expect(financeWork(items, "team").map(item => item.id)).toEqual(["other"]);
  });

  it("keeps request waiting and routing blockers visible in their own views", () => {
    const items = [row("requested", { queue: "waiting", canAct: false, relationships: ["requested"] }), row("routing", { queue: "waiting", canAct: false, category: "setup", relationships: [], blocker: "Assign an independent reviewer" })];
    expect(financeWork(items, "waiting").map(item => item.id)).toEqual(["requested"]);
    expect(financeWork(items, "team").map(item => item.id)).toEqual(["routing"]);
  });

  it("counts one source identity when normal responsibility and a flow override both arrive",()=>{
    const mine=row("same"), override=row("same",{nextAction:"Override responsibility"});
    expect(financeWork([mine,override],"actions")).toHaveLength(1);
  });
  it("searches the next action and orders due work before undated work", () => {
    const items = [row("undated", { nextAction: "Confirm beneficiary" }), row("due", { nextAction: "Confirm beneficiary", dueAt: 1 }), row("unrelated")];
    expect(financeWork(items, "actions", " beneficiary ").map(item => item.id)).toEqual(["due", "undated"]);
  });
});
