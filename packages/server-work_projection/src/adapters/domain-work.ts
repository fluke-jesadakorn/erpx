import {ApiError} from "@erpx/server-runtime/public/http";
import "server-only";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { listTasks, taskWorkState } from "@erpx/server-operations/public/staff-tasks";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { listExpenses, canEditExpense, canReviewExpense, canPostExpenses } from "@erpx/server-people_payroll/public/expenses";
import { listCounts, canReadCounts, canEditCount, canReviewCount } from "@erpx/server-inventory/public/inventory-counts";
import {countSourceAccess} from "@erpx/server-inventory/public/count-purchases";
import {transferSourceAccess} from "@erpx/server-inventory/public/transfer-purchases";
import { listTransfers, canEditTransfer, canReviewTransfer } from "@erpx/server-inventory/public/inventory-transfers";
import { createDomainWork, type DomainWorkPorts } from "@erpx/server-work_projection/application/domain-work";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function domainWorkPorts(db: SqlDatabase): DomainWorkPorts {
    const countAccess=new WeakMap<WorkContext,Map<string,Awaited<ReturnType<typeof countSourceAccess>>>>();
    const transferAccess=new WeakMap<WorkContext,Awaited<ReturnType<typeof transferSourceAccess>>>();
    return { listTasks: (...args) => listTasks(db, ...args),
        taskWorkState: (...args) => taskWorkState(db, ...args),
        listExpenses,
        canEditExpense,
        canReviewExpense,
        canPostExpenses,
        listCounts:async actor=>{try{const rows=await listCounts(actor),access=new Map<string,Awaited<ReturnType<typeof countSourceAccess>>>();for(const row of rows)access.set(String(row.id),await countSourceAccess(db,actor,String(row.id)));countAccess.set(actor,access);return rows;}catch(error){if(error instanceof ApiError&&[403,404].includes(error.status))return [];throw error;}},
        canReadCounts,
        canEditCount:(actor,row)=>canEditCount(actor,row)&&(countAccess.get(actor)?.get(String(row.id))?.newWork??true),
        canReviewCount:(actor,row)=>canReviewCount(actor,row)&&(countAccess.get(actor)?.get(String(row.id))?.settleExisting??true),
        listTransfers:async actor=>{const access=await transferSourceAccess(db,actor);transferAccess.set(actor,access);return access&&!access.history?[]:listTransfers(actor);},
        canEditTransfer:(actor,row)=>canEditTransfer(actor,row)&&(transferAccess.get(actor)?.newWork??true),
        canReviewTransfer:(actor,row)=>canReviewTransfer(actor,row)&&(transferAccess.get(actor)?.settleExisting??true) };
}
export async function getDomainWork(actor: WorkContext, source?: "expense" | "stock_count" | "transfer" | "staff_task") { const { write } = await holdingResources(actor.holdingId); return createDomainWork(domainWorkPorts(write)).getDomainWork(actor, source); }
export type { DomainWork } from "@erpx/server-work_projection/application/domain-work";
export async function getStaffWork(actor: WorkContext) { const { write } = await holdingResources(actor.holdingId); return createDomainWork(domainWorkPorts(write)).getStaffWork(actor); }
