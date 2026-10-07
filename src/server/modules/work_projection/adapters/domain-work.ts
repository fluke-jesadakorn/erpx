import "server-only";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { listTasks, taskWorkState } from "@/server/modules/operations/public/staff-tasks";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { listExpenses, canEditExpense, canReviewExpense, canPostExpenses } from "@/server/modules/people_payroll/public/expenses";
import { listCounts, canReadCounts, canEditCount, canReviewCount } from "@/server/modules/inventory/public/inventory-counts";
import { listTransfers, canEditTransfer, canReviewTransfer } from "@/server/modules/inventory/public/inventory-transfers";
import { createDomainWork, type DomainWorkPorts } from "../application/domain-work";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
export function domainWorkPorts(db: SqlDatabase): DomainWorkPorts {
    return { listTasks: (...args) => listTasks(db, ...args),
        taskWorkState: (...args) => taskWorkState(db, ...args),
        listExpenses,
        canEditExpense,
        canReviewExpense,
        canPostExpenses,
        listCounts,
        canReadCounts,
        canEditCount,
        canReviewCount,
        listTransfers,
        canEditTransfer,
        canReviewTransfer };
}
export async function getDomainWork(actor: WorkContext, source?: "expense" | "stock_count" | "transfer" | "staff_task") { const { write } = await holdingResources(actor.holdingId); return createDomainWork(domainWorkPorts(write)).getDomainWork(actor, source); }
export type { DomainWork } from "../application/domain-work";
export async function getStaffWork(actor: WorkContext) { const { write } = await holdingResources(actor.holdingId); return createDomainWork(domainWorkPorts(write)).getStaffWork(actor); }
