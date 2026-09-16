import type { WorkContext } from "./authorization";
import type { FlowKind } from "@/lib/flow";
import { holdingResources } from "./runtime";
import { flowRecord } from "./flow-routing";

/** Reuse the owner's read policy; the workflow endpoint never grants document access. */
export async function requireFlowAccess(actor: WorkContext, kind: FlowKind, id: string) {
  const { write } = await holdingResources(actor.holdingId);
  switch (kind) {
    case "expense": await (await import("./expenses")).getExpense(actor,id); break;
    case "leave": await (await import("./leave")).getLeave(actor.holdingId,id,actor); break;
    case "staff_task": await (await import("./staff-tasks")).taskDetail(write,actor,id); break;
    case "request": await (await import("./employee-requests")).getEmployeeRequest(write,actor,id); break;
    case "stock_count": await (await import("./inventory-counts")).getCount(actor,id); break;
    case "transfer": await (await import("./inventory-transfers")).getTransfer(actor,id); break;
    case "order": await (await import("./commerce")).getOrder(actor,id); break;
    case "invoice": await (await import("./finance")).getInvoice(actor,id); break;
    case "journal": await (await import("./ledger")).journalDetail(actor,id); break;
    case "payroll": await (await import("./people-payroll")).getPayrollRun(actor,id); break;
    case "record": { const row = await flowRecord(write,actor.companyId,kind,id); await (await import("./records")).getRecord(actor,String(row.module_id),id); break; }
  }
  return write;
}
