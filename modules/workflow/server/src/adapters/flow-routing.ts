import {requireLegacyFinanceJournal,requireLegacyFinanceInvoiceFlow} from '@erpx/finance-server/public/document-purchases';
import {requireExpensePurchasedHistory,expenseDocumentFlow,withExpenseFlowAssignment} from "@erpx/people-payroll-server/public/expense-purchases";
import {requireWorkforceFlowHistory,workforceDocumentFlow,withWorkforceFlowAssignment} from "@erpx/people-payroll-server/public/workforce-record-purchases";

import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";

import type { FlowKind, FlowView } from "@erpx/platform-shared-ui/lib/flow";

import { type StaffActor } from "@erpx/operations-server/public/staff-policy";

import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '@erpx/workflow-server/public/commands';
import { ApiError as NativeApiError } from '@erpx/platform-runtime-server/public/http';

import { createFlowRoutingApplication } from '@erpx/workflow-server/application/flow-routing';
import { BusinessRuleError } from '@erpx/platform-shared-server/index';
import { payrollDocumentFlow, withPayrollPurchasedCommand } from '@erpx/people-payroll-server/public/payroll-purchases';
import {requireCommerceFlowHistory,commerceDocumentFlow,withCommerceFlowAssignment} from '@erpx/commerce-server/public/workflow-purchases';
import {requireCountPurchasedRead,countDocumentFlow,withCountPurchasedCommand} from '@erpx/inventory-server/public/count-purchases';
import {requireTransferPurchasedRead,transferDocumentFlow,withTransferPurchasedCommand} from '@erpx/inventory-server/public/transfer-purchases';
import { nativeFlowPlanningPorts } from '@erpx/workflow-server/adapters/flow-planning-ports';
function application(database: SqlDatabase) {
    return createFlowRoutingApplication({ ...nativeFlowPlanningPorts(database), commands: {
        replay: (...args) => nativeReplayCommand(database, ...args),
        commit: (...args) => nativeExecuteCommand(database, ...args),
    } });
}
async function invoke<Result>(run: () => Promise<Result>) { try {
    return await run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new NativeApiError(error.status, error.code, error.message);
    throw error;
} }



export async function resolveFlow(db: SqlDatabase, actor: StaffActor, kind: FlowKind, id: string): Promise<FlowView> {
    if(kind==='invoice')await requireLegacyFinanceInvoiceFlow(db,actor,id);
    if(kind==='journal')await requireLegacyFinanceJournal(db,actor,id);
    if(kind === "expense")await requireExpensePurchasedHistory(db,actor);
    if(kind === "leave")await requireWorkforceFlowHistory(db,actor,id);
    if(kind === "order")await requireCommerceFlowHistory(db,actor,id);
    if(kind === "stock_count")await requireCountPurchasedRead(db,actor,id);
    if(kind === "transfer")await requireTransferPurchasedRead(db,actor,{kind:"transfer",id});
    const view = await invoke(() => application(db).resolveFlow(actor, kind, id));
    return kind === "expense" ? expenseDocumentFlow(db,actor,view) : kind === "payroll" ? payrollDocumentFlow(db, actor, view) : kind === "order" ? commerceDocumentFlow(db,actor,view) : kind === "stock_count" ? countDocumentFlow(db,actor,view) : kind === "transfer" ? transferDocumentFlow(db,actor,view) : kind === "leave" ? workforceDocumentFlow(db,actor,view) : view;
}
export async function assignFlowPrimary(db: SqlDatabase, actor: StaffActor, kind: FlowKind, id: string, input: {
    stepKey: string;
    userId: string;
    expectedVersion: number;
    recordVersion: number;
    reason: string;
    saveDefault: boolean;
}, key: string): Promise<Record<string, unknown>> {
    if(kind==='invoice')await requireLegacyFinanceInvoiceFlow(db,actor,id);
    if(kind==='journal')await requireLegacyFinanceJournal(db,actor,id);
    const run=(database:SqlDatabase)=>invoke(() => application(database).assignFlowPrimary(actor, kind, id, input, key));
    return kind === "expense" ? withExpenseFlowAssignment(db,actor,id,{key,stepKey:input.stepKey,saveDefault:input.saveDefault},run) : kind === "payroll" ? withPayrollPurchasedCommand(actor, {key,operation:"flow.assign",purpose:"settle-existing"}, tx => run(tx ?? db), db)
        : kind === "order" ? withCommerceFlowAssignment(db,actor,id,{key,stepKey:input.stepKey,saveDefault:input.saveDefault},run)
        : kind === "stock_count" ? withCountPurchasedCommand(db,actor,{key,operation:"flow.assign",id,purpose:"settle-existing",permission:"inventory.read",existing:true,flowStep:input.stepKey,saveDefault:input.saveDefault},run)
        : kind === "leave" ? withWorkforceFlowAssignment(db,actor,id,{key,stepKey:input.stepKey,saveDefault:input.saveDefault},run)
        : kind === "transfer" ? withTransferPurchasedCommand(db,actor,{key,operation:"flow.assign",target:{kind:"transfer",id},purpose:"settle-existing",permissions:[],existing:true,flowStep:input.stepKey,saveDefault:input.saveDefault},run) : run(db);
}
