import { workflowFinalOwnerProjections as commerceFinalOwnerProjections } from '@erpx/server-commerce/public/workflow-final-owner-projections';
import {workflowWave3Statement} from './native-workflow-wave3';
import {nativeWorkflowReadAll,nativeWorkflowReadFirst} from './native-workflow-read';
import {approvalStepsByInstanceSql} from './approval-projection-sources';
import {tableExistsGuardSql} from './closed-projection-sources';
import flowAdvanceContract from '../flow-advance-sql.json' with { type: 'json' };
import { workflowFinalOwnerProjections as financeFinalOwnerProjections } from '@erpx/server-finance/public/workflow-final-owner-projections';
import { workflowFinalOwnerProjections as peoplepayrollFinalOwnerProjections } from '@erpx/server-people_payroll/public/workflow-final-owner-projections';
const commerceFinalSources=commerceFinalOwnerProjections(),financeFinalSources=financeFinalOwnerProjections(),peoplepayrollFinalSources=peoplepayrollFinalOwnerProjections();
/** Byte-identical span shared by the order flow's stock_gl and invoicing branches; emitted SQL is unchanged. */
const orderFulfilledExistsSql = `WHEN d.state='fulfilled' AND EXISTS(SELECT 1 ${commerceFinalSources.flow_routing_commerce_fulfillments_f_from} `;
/** Byte-identical span shared by the flow-defaults revision read and the route-version guard; emitted SQL is unchanged. */
const flowDefaultsMaxRevisionSql = "SELECT COALESCE(MAX(revision),0)";
const flowAdvanceSql: Readonly<Partial<Record<string,string>>> = flowAdvanceContract;
function flowAdvanceStatement(kind:FlowKind):string{const sql=flowAdvanceSql[kind];if(!sql)throw new Error(`FLOW_ADVANCE_SQL_MISSING:${kind}`);return sql;}



import { createFlowRoutingSourceRepository as commerce_flow_routing } from '@erpx/server-commerce/public/workflow-flow-routing-facts';
import { createFlowRoutingSourceRepository as finance_flow_routing } from '@erpx/server-finance/public/workflow-flow-routing-facts';
import { createFlowRoutingSourceRepository as operations_flow_routing } from '@erpx/server-operations/public/workflow-flow-routing-facts';
import { createFlowRoutingSourceRepository as support_flow_routing } from '@erpx/server-support/public/workflow-flow-routing-facts';
import { createFlowRoutingSourceRepository as records_flow_routing } from '@erpx/server-records/public/workflow-flow-routing-facts';
import { createFlowRoutingSourceRepository as people_payroll_flow_routing } from '@erpx/server-people_payroll/public/workflow-flow-routing-facts';
import { createFlowRoutingSourceRepository as identity_access_flow_routing } from '@erpx/server-identity_access/public/workflow-flow-routing-facts';
import { stageWorkflowNotification } from '@erpx/server-collaboration/adapters/native-workflow-notification';
import { stageWorkflowInvoicingNotification } from '@erpx/server-collaboration/adapters/native-workflow-invoicing-notification';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { stageWorkflowPostedOrderRevision } from '@erpx/server-commerce/public/workflow-command-effects';
import { workflowRecord } from '@erpx/server-workflow/adapters/document-sources';
import {approvalView} from '@erpx/server-workflow/public/approval-planning';
import {canOverrideEmployeeRequest,readRequestForOverride} from '@erpx/server-records/public/request-override-read';
import { assertProductionModuleEnabled, productionFeatureGuard } from "@erpx/server-pms/public/feature-availability";
import { hasPolicyReadSnapshot } from "@erpx/server-identity_access/public/iam-read-snapshot";
import { currentPersonPrincipals, personIdentityIds } from "@erpx/server-identity_access/public/iam-person";

import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import { isTransactionalSqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { FlowKind } from "@erpx/feature-shared/lib/flow";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { companyAuthority, authorityGuard, notification, type StaffActor } from "@erpx/server-operations/public/staff-policy";
import { expenseRoleUsers, expenseRoute, expensePoolCandidates, expenseAuthoritySnapshot, spendingStepCandidates, spendingUsers } from "@erpx/server-people_payroll/public/expense-routing";
import type {ApprovalInstance,ApprovalStep} from '@erpx/server-workflow/domain/approvals';
import {resolveServiceTeam,teamHandlers} from '@erpx/server-support/public/service-team-reads';
import {peopleRequestRoute} from '@erpx/server-organization/public/department-authority-reads';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '@erpx/server-identity_access/public/iam-read-snapshot';
import { ApiError as NativeApiError } from '@erpx/server-runtime/public/http';
import { BusinessRuleError } from '@erpx/server-shared/index';
import type { Route, DefaultRow, FlowHistoryRow } from '@erpx/server-workflow/domain/flow-routing';
import type { FlowPlanningPorts, FlowRoutingRepository } from '@erpx/server-workflow/application/flow-routing';
async function flowAfterEffects(db: SqlDatabase, actor: StaffActor, key: string, kind: FlowKind, id: string): Promise<SqlStatement[]> {
    const stateColumn = ["leave", "staff_task", "request", "order"].includes(kind) ? "state" : "status";
    const approvalIndex = `(SELECT current_step FROM approval_instances WHERE company_id=d.company_id AND record_id=d.id ORDER BY insertion_order DESC LIMIT 1)`;
    const state = kind === "expense" ? `CASE WHEN d.workflow_stage='department_approval' AND EXISTS(SELECT 1 FROM approval_steps s JOIN approval_instances i ON i.id=s.instance_id WHERE i.record_type='expense' AND i.record_id=d.id AND i.company_id=d.company_id AND s.ordinal=${approvalIndex} AND jsonb_exists(s.route_snapshot_json::jsonb,'departmentPolicyVersion')) THEN 'department_approval_' || (${approvalIndex})::text WHEN d.workflow_version=2 THEN d.workflow_stage ELSE CASE d.status WHEN 'in_review' THEN 'department_approval' WHEN 'approved' THEN 'gl_posting' WHEN 'posted' THEN 'payment' WHEN 'rejected' THEN 'draft' ELSE d.status END END` : kind === "leave" ? `CASE WHEN d.state='pending_approval' THEN 'approval_' || (${approvalIndex})::text ELSE d.state END` : kind === "staff_task" ? `CASE WHEN d.state IN ('ready','in_progress','rework') THEN 'work' WHEN d.state='start_approval' THEN 'start_' || (${approvalIndex})::text WHEN d.state='completion_approval' THEN 'completion_' || (${approvalIndex})::text ELSE d.state END` : kind === "request" ? "CASE WHEN d.state IN ('resolved','fulfilled') THEN 'confirm' WHEN d.state='returned' THEN 'draft' WHEN d.state='awaiting_approval' AND d.approval_json::jsonb #>> '{route,steps,0,snapshot,departmentPolicyVersion}' IS NOT NULL THEN 'purchase_' || COALESCE((d.approval_json::jsonb ->> 'currentStep')::int,0)::text WHEN d.state='awaiting_approval' AND d.approval_json::jsonb #>> '{route,steps,0,snapshot,spendingPolicy}' IS NOT NULL THEN 'purchase_' || (d.approval_json::jsonb #> '{route,steps}' -> COALESCE((d.approval_json::jsonb ->> 'currentStep')::int,0) ->> 'stage') ELSE d.state END" : kind === "invoice" ? "CASE WHEN d.status='partially_paid' THEN 'posted' ELSE d.status END" : kind === "order" ? `CASE WHEN d.state='partially_fulfilled' THEN 'confirmed' ${orderFulfilledExistsSql}${financeFinalSources.flow_routing_finance_journals_j_join} ON j.id=f.journal_id WHERE f.order_id=d.id AND j.status IN ('draft','in_review')) THEN 'stock_gl' ${orderFulfilledExistsSql}WHERE f.order_id=d.id AND f.invoice_id IS NULL) THEN 'invoicing' ELSE d.state END` : kind === "payroll" ? `CASE WHEN d.status='posted' THEN COALESCE((SELECT CASE status WHEN 'draft' THEN 'payment_draft' WHEN 'approved' THEN 'payment_approved' WHEN 'exported' THEN 'payment_exported' WHEN 'partially_settled' THEN 'payment_partially_settled' WHEN 'settled' THEN 'payment_settled' ELSE 'posted' END ${peoplepayrollFinalSources.flow_routing_hr_payment_batches_from} WHERE company_id=d.company_id AND source_type='payroll' AND source_id=d.id AND status<>'cancelled' ORDER BY created_at DESC,id DESC LIMIT 1),'posted') ELSE d.status END` : `d.${stateColumn}`;
    const recipient = kind === "request" ? `CASE WHEN d.state='in_progress' THEN d.assignee_user_id ELSE r.steps_json::jsonb -> (${state}) ->> 'primaryUserId' END` : `r.steps_json::jsonb -> (${state}) ->> 'primaryUserId'`;
    const after = [await stageWorkflowNotification(db, actor as WorkContext, { id: crypto.randomUUID(), kind, now: Date.now(), companyId: actor.companyId, recordId: id, actorId: actor.userId, documentTable: tables[kind], recipientSql: recipient })];
    // Rust owns the per-kind statement; this adapter only binds it.
    after.push(db.prepare(flowAdvanceStatement(kind)).bind(kind, key, actor.companyId, id));
    if (kind === 'journal')
        after.unshift(stageWorkflowPostedOrderRevision(Date.now(), id, actor.companyId, actor.userId, key));
    if (kind === 'journal')
        after.push(await stageWorkflowInvoicingNotification(db, actor as WorkContext, crypto.randomUUID(), Date.now(), id));
    return after;
}
const tables: Record<FlowKind, string> = { expense: "finance_expenses", leave: "leave_requests", staff_task: "staff_tasks", request: "employee_requests", stock_count: "inventory_counts", transfer: "inventory_transfers", order: "commerce_orders", invoice: "finance_invoices", journal: "finance_journals", payroll: "hr_payroll_runs", record: "workspace_records" };
/** Byte-identical company-scoped version-guard family: the closed target selects
 * only the source table and key predicate; emitted SQL and bind order stay exact. */
const flowVersionGuardSql = (target: 'route' | FlowKind): string => {
    const table = target === 'route' ? 'flow_routes' : tables[target];
    const key = target === 'route' ? 'kind=? AND record_id=?' : 'id=?';
    return tableExistsGuardSql(table, `company_id=? AND ${key} AND version=?)`);
};
export function nativeFlowPlanningPorts(database: SqlDatabase): FlowPlanningPorts<SqlStatement> {
    const repository: FlowRoutingRepository<SqlStatement> = { async readFlowRecordRow(companyId: string, id: string, kind: FlowKind) { const row = await workflowRecord(database, kind, companyId, id); if (row && kind === "order") return { ...row, has_service_lines: await commerce_flow_routing(database).readServiceOrder(companyId,id) ? 1 : 0 }; return row; },
        readWorkFulfillmentsRows(companyId: string) { return commerce_flow_routing(database).readWorkFulfillmentsRows(companyId); },
        readDefinitionsInstance(company_id: string, approvalKind: "expense" | "leave" | "request" | "stock_count" | "transfer" | "order" | "invoice" | "journal" | "payroll" | "record" | "task", id: string | number | null) { return nativeWorkflowReadFirst<ApprovalInstance>('wf_native_flow_definitions_instance', [company_id, approvalKind, id], company_id, database); },
        readDefinitionsApprovalSteps(id: string) { return database.prepare(approvalStepsByInstanceSql).bind(id).all<ApprovalStep>(); },
        readDefinitionsJournal(journal_id: string | number, company_id: string) { return finance_flow_routing(database).readDefinitionsJournal(journal_id, company_id); },
        readDefinitionsDelegation(company_id: string, id: string | number | null) { return operations_flow_routing(database).readDefinitionsDelegation(company_id, id); },
        readDefinitionsTeam(company_id: string, teamId: string) { return support_flow_routing(database).readDefinitionsTeam(company_id, teamId); },
        readDefinitionsPending(id: string | number | null) { return commerce_flow_routing(database).readDefinitionsPending(id); },
        readDefinitionsReady(id: string | number | null) { return commerce_flow_routing(database).readDefinitionsReady(id); },
        readDefinitionsStockGl(id: string | number | null) { return commerce_flow_routing(database).readDefinitionsStockGl(id); },
        // Only read-only policy snapshots share company-scoped source facts.
        // Commands keep their individual reads on the transaction client.
        readDefinitionsRequest(source_id: string | number | null, company_id: string) { if (!hasPolicyReadSnapshot()) return records_flow_routing(database).readDefinitionsRequest(source_id, company_id);
            return nativePolicyRead(database, JSON.stringify(["flow-source-requests",company_id]), async () => {
                const rows = (await records_flow_routing(database).readWorkflowRequests(company_id)).results;
                const bySource = new Map<string, typeof rows[number]>();
                for (const row of rows) if (!bySource.has(row.source_id)) bySource.set(row.source_id,row);
                return bySource;
            }).then(rows => rows.get(String(source_id)) ?? null); },
        readDefinitionsJournal2(journal_id: string | number, company_id: string) { if (!hasPolicyReadSnapshot()) return finance_flow_routing(database).readDefinitionsJournal2(journal_id, company_id);
            return nativePolicyRead(database, JSON.stringify(["flow-source-journals",company_id]), async () => new Map((await finance_flow_routing(database).readWorkflowJournals(company_id)).results.map(row => [row.id,row]))).then(rows => rows.get(String(journal_id)) ?? null); },
        readDefinitionsDelivery(source_id: string | number | null) { return commerce_flow_routing(database).readDefinitionsDelivery(source_id); },
        readDefinitionsPaymentBatch(company_id: string, id: string | number | null) { if (!hasPolicyReadSnapshot()) return people_payroll_flow_routing(database).readDefinitionsPaymentBatch(company_id, id).then(batch => batch ?? null);
            return nativePolicyRead(database, JSON.stringify(["flow-payment-batches",company_id]), async () => new Map((await people_payroll_flow_routing(database).readWorkflowPaymentBatches(company_id)).map(row => [row.source_id,row]))).then(rows => rows.get(String(id)) ?? null); },
        async readSharedFlowReferencesRoutesdefaultsevents(companyId: string) { return {results: await nativeWorkflowReadAll<Route & {
            kind: string;
            record_id: string;
        }>('wf_native_flow_routes_by_company', [companyId], companyId, database)}; },
        async readSharedFlowReferencesRoutesdefaultsevents2(companyId: string) { return {results: await nativeWorkflowReadAll<DefaultRow & {
            kind: string;
            branch_id: string | null;
            department_id: string | null;
        }>('wf_native_flow_defaults_by_company', [companyId], companyId, database)}; },
        async readSharedFlowReferencesRoutesdefaultsevents3(companyId: string) { return {results: await nativeWorkflowReadAll<FlowHistoryRow>('wf_native_flow_events_history', [companyId], companyId, database)}; },
        async readReadFlowRoute(companyId: string, kind: "expense" | "leave" | "staff_task" | "request" | "stock_count" | "transfer" | "order" | "invoice" | "journal" | "payroll" | "record", id: string) { const rows = await nativeWorkflowReadAll<Route>('wf_native_flow_routes_by_company_kind_record', [companyId, kind, id], companyId, database); return rows[0] ?? null; },
        async readReadFlowDefaultsRevision(companyId: string) { return nativeWorkflowReadFirst<{
            revision: number;
        }>('wf_native_flow_defaults_revision', [companyId], companyId, database); },
        async readReadFlowDefaults(companyId: string, kind: "expense" | "leave" | "staff_task" | "request" | "stock_count" | "transfer" | "order" | "invoice" | "journal" | "payroll" | "record", branch_id: string | null, department_id: string | null, defaultsRevision: number) { return {results: await nativeWorkflowReadAll<DefaultRow>('wf_native_flow_defaults_by_kind_revision', [companyId, kind, branch_id, department_id, defaultsRevision], companyId, database)}; },
        readReadFlowPeople(companyId: string, now: number, now2: number) { return people_payroll_flow_routing(database).readReadFlowPeople(companyId, now, now2); },
        readReadFlowMembers(companyId: string, now: number, now2: number) { return identity_access_flow_routing(database).readReadFlowMembers(companyId, now, now2); },
        async readReadFlowHistoryRows(companyId: string, kind: "expense" | "leave" | "staff_task" | "request" | "stock_count" | "transfer" | "order" | "invoice" | "journal" | "payroll" | "record", id: string) { return {results: await nativeWorkflowReadAll<{
            action: string;
            actor_name: string;
            created_at: number;
            reason: string;
            step_key: string;
        }>('wf_native_flow_events_record_history', [companyId, kind, id], companyId, database)}; },
        guard24(key: string, value: string, args: string[], version: number) { return assertion(database, key, value, flowVersionGuardSql('route'), [...args, version]); },
        guard25(key: string, value: string, companyId: string, revision: number) { return assertion(database, key, value, `(${flowDefaultsMaxRevisionSql} FROM flow_defaults WHERE company_id=?)=?`, [companyId, revision]); },
        async stageRouteStatementsEffect(args: string[], revision: number, value: string, now: number) {
            return workflowWave3Statement(database, {kind:'system'}, 'wf_flow_stageroutestatementseffect', [...args, revision, value, now], String(now).slice(0,180), now); },
        readPrepareFlowCommandPerson(companyId: string, userId: string) { return people_payroll_flow_routing(database).readPrepareFlowCommandPerson(companyId, userId); },
        async stagePrepareFlowCommandSteps(value: string, key: string, companyId: string, kind: "expense" | "leave" | "staff_task" | "request" | "stock_count" | "transfer" | "order" | "invoice" | "journal" | "payroll" | "record", id: string, value2: string, action: string, userId: string, value3: string, reason: string, version: number, now: number) {
            return workflowWave3Statement(database, {kind:'system'}, 'wf_flow_stageprepareflowcommandsteps', [value, key, companyId, kind, id, value2, action, userId, value3, reason, version, now], String(key).slice(0,180), now); },
        readPrepareFlowCommandParent(id: string, companyId: string) { return commerce_flow_routing(database).readPrepareFlowCommandParent(id, companyId); },
        async readAssignFlowPrimaryExisting(companyId: string, kind: "expense" | "leave" | "staff_task" | "request" | "stock_count" | "transfer" | "order" | "invoice" | "journal" | "payroll" | "record", id: string) { return nativeWorkflowReadFirst<{
            steps_json: string;
        }>('wf_native_flow_routes_steps_json', [companyId, kind, id], companyId, database); },
        readAssignFlowPrimaryParent(id: string, companyId: string) { return commerce_flow_routing(database).readAssignFlowPrimaryParent(id, companyId); },
        async stageAssignFlowPrimaryEffect(userId: string, companyId: string, order_id: string) {
            return workflowWave3Statement(database, {kind:'system'}, 'wf_flow_stageassignflowprimaryeffect', [userId, companyId, order_id], String(companyId).slice(0,180), Date.now()); },
        readAssignFlowPrimaryAdministrator(companyId: string, userId: string) { return people_payroll_flow_routing(database).readAssignFlowPrimaryAdministrator(companyId, userId); },
        guard34(key: string, value: "flow-document-version", companyId: string, id: string, recordVersion: number, kind: FlowKind) { return assertion(database, key, value, flowVersionGuardSql(kind), [companyId, id, recordVersion]); },
        async stageAssignFlowPrimaryEffect2(value: string, companyId: string, kind: "expense" | "leave" | "staff_task" | "request" | "stock_count" | "transfer" | "order" | "invoice" | "journal" | "payroll" | "record", id: string) {
            return workflowWave3Statement(database, {kind:'system'}, 'wf_flow_stageassignflowprimaryeffect2', [value, companyId, kind, id], String(companyId).slice(0,180), Date.now()); },
        async stageAssignFlowPrimaryEffect3(companyId: string, kind: "expense" | "leave" | "staff_task" | "request" | "stock_count" | "transfer" | "order" | "invoice" | "journal" | "payroll" | "record", key: string, branch_id: string | null, department_id: string | null, userId: string, userId2: string, reason: string, now: number) {
            return workflowWave3Statement(database, {kind:'system'}, 'wf_flow_stageassignflowprimaryeffect3', [companyId, kind, key, branch_id, department_id, userId, userId2, reason, now], String(key).slice(0,180), now); },
        async stageAssignFlowPrimaryEffect4(value: string, key: string, companyId: string, kind: "expense" | "leave" | "staff_task" | "request" | "stock_count" | "transfer" | "order" | "invoice" | "journal" | "payroll" | "record", id: string, key2: string, userId: string, value2: string, value3: string, recordVersion: number, now: number) {
            return workflowWave3Statement(database, {kind:'system'}, 'wf_flow_stageassignflowprimaryeffect4', [value, key, companyId, kind, id, key2, userId, value2, value3, recordVersion, now], String(key).slice(0,180), now); },
    };
    return { repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, prepare: async () => {}, production: process.env.NODE_ENV === 'production', batch: effects => database.batch(effects), policyRead: (key, read) => nativePolicyRead(database, key, read), policyNow: nativePolicyNow, flowAfterEffects: (...args) => flowAfterEffects(database, ...args), approvalView: (...args) => fromNative(() => approvalView(database, ...args)), canOverrideEmployeeRequest: (...args) => fromNative(() => canOverrideEmployeeRequest(database, ...args)), readRequestForOverride: (...args) => fromNative(() => readRequestForOverride(database, ...args)), assertProductionModuleEnabled: (...args) => fromNative(() => assertProductionModuleEnabled(database, ...args)), productionFeatureGuard: (...args) => productionFeatureGuard(database, ...args), hasPolicyReadSnapshot: (...args) => fromNativeSync(() => hasPolicyReadSnapshot(...args)), currentPersonPrincipals: (...args) => fromNative(() => currentPersonPrincipals(database, ...args)), personIdentityIds: (...args) => fromNative(() => personIdentityIds(database, ...args)), isTransactionalSqlDatabase: (...args) => isTransactionalSqlDatabase(database, ...args), companyAuthority: (...args) => fromNative(() => companyAuthority(database, ...args)), authorityGuard: (...args) => authorityGuard(database, ...args), notification: (...args) => notification(database, ...args), expenseRoleUsers: (...args) => fromNative(() => expenseRoleUsers(database, ...args)), expenseRoute: (...args) => fromNative(() => expenseRoute(database, ...args)), expensePoolCandidates: (...args) => fromNative(() => expensePoolCandidates(database, ...args)), expenseAuthoritySnapshot: (...args) => fromNative(() => expenseAuthoritySnapshot(database, ...args)), spendingStepCandidates: (...args) => fromNative(() => spendingStepCandidates(database, ...args)), spendingUsers: (...args) => fromNative(() => spendingUsers(database, ...args)), resolveServiceTeam: (...args) => fromNative(() => resolveServiceTeam(database, ...args)), teamHandlers: (...args) => fromNative(() => teamHandlers(database, ...args)), peopleRequestRoute: (...args) => fromNative(() => peopleRequestRoute(database, ...args)) };
}
async function fromNative<Result>(run: () => Promise<Result> | Result): Promise<Result> { try {
    return await run();
}
catch (error) {
    if (error instanceof NativeApiError)
        throw new BusinessRuleError(error.status, error.code, error.message);
    throw error;
} }
function fromNativeSync<Result>(run: () => Result): Result { try {
    return run();
}
catch (error) {
    if (error instanceof NativeApiError)
        throw new BusinessRuleError(error.status, error.code, error.message);
    throw error;
} }
