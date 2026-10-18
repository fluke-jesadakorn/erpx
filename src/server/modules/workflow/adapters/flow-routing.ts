import {requireExpensePurchasedHistory,expenseDocumentFlow,withExpenseFlowAssignment} from "../../people_payroll/public/expense-purchases";
import {requireWorkforceFlowHistory,workforceDocumentFlow,withWorkforceFlowAssignment} from "../../people_payroll/public/workforce-record-purchases";
import {workflowFinalOwnerProjections as commerceFinalOwnerProjections} from "../../commerce/public/workflow-final-owner-projections";
const commerceFinalSources=commerceFinalOwnerProjections();
import {workflowFinalOwnerProjections as financeFinalOwnerProjections} from "../../finance/public/workflow-final-owner-projections";
const financeFinalSources=financeFinalOwnerProjections();
import {workflowFinalOwnerProjections as peoplepayrollFinalOwnerProjections} from "../../people_payroll/public/workflow-final-owner-projections";
const peoplepayrollFinalSources=peoplepayrollFinalOwnerProjections();
import { createFlowRoutingSourceRepository as commerce_flow_routing } from '../../commerce/public/workflow-flow-routing-facts';
import { createFlowRoutingSourceRepository as finance_flow_routing } from '../../finance/public/workflow-flow-routing-facts';
import { createFlowRoutingSourceRepository as operations_flow_routing } from '../../operations/public/workflow-flow-routing-facts';
import { createFlowRoutingSourceRepository as support_flow_routing } from '../../support/public/workflow-flow-routing-facts';
import { createFlowRoutingSourceRepository as records_flow_routing } from '../../records/public/workflow-flow-routing-facts';
import { createFlowRoutingSourceRepository as people_payroll_flow_routing } from '../../people_payroll/public/workflow-flow-routing-facts';
import { createFlowRoutingSourceRepository as identity_access_flow_routing } from '../../identity_access/public/workflow-flow-routing-facts';
import { stageWorkflowNotification, stageWorkflowInvoicingNotification } from '../../collaboration/public/workflow-notification-effects';
import { stageWorkflowPostedOrderRevision } from '../../commerce/public/workflow-command-effects';
import { workflowRecord } from './document-sources';
import { approvalView } from "@/server/modules/workflow/adapters/approvals";
import { canOverrideEmployeeRequest, readRequestForOverride } from "@/server/modules/records/public/employee-requests";
import { assertProductionModuleEnabled, productionFeatureGuard } from "@/server/modules/pms/public/hospitality-rollout";
import { hasPolicyReadSnapshot } from "@/server/modules/identity_access/public/iam-read-snapshot";
import { currentPersonPrincipals, personIdentityIds } from "@/server/modules/identity_access/public/iam-person";
import type { PermissionActor } from "@/lib/identity_access/iam";
import type { SqlDatabase, SqlStatement } from "@/server/modules/runtime/public/runtime/types";
import { isTransactionalSqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import type { FlowKind, FlowStep, FlowView } from "@/lib/shared/flow";
import { assertion } from "@/server/modules/workflow/adapters/commands";
import { companyAuthority, authorityGuard, notification, type StaffActor } from "@/server/modules/operations/public/staff-policy";
import { expenseRoleUsers, expenseRoute, expensePoolCandidates, expenseAuthoritySnapshot, spendingStepCandidates, spendingUsers } from "@/server/modules/people_payroll/public/expense-routing";
import type { ApprovalInstance, ApprovalStep } from "@/server/modules/workflow/adapters/approvals";
import { resolveServiceTeam, teamHandlers } from "@/server/modules/support/public/service-teams";
import { peopleRequestRoute } from "@/server/modules/organization/public/department-authority";
import { holdingResources as nativeHoldingResources } from '../../runtime/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '../../identity_access/public/iam-read-snapshot';
import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '../../workflow/public/commands';
import { ApiError as NativeApiError } from '../../runtime/public/http';
import { type Row, type Route, type DefaultRow, flowRoleAllowed as pureflowRoleAllowed, type FlowHistoryRow, flowCommandTarget as pureflowCommandTarget } from '../domain/flow-routing';
import { createFlowRoutingApplication, type FlowRoutingRepository } from '../application/flow-routing';
import { BusinessRuleError } from '../../shared';
import { payrollDocumentFlow, withPayrollPurchasedCommand } from '../../people_payroll/public/payroll-purchases';
import {requireCommerceFlowHistory,commerceDocumentFlow,withCommerceFlowAssignment} from '../../commerce/public/workflow-purchases';
import {requireCountPurchasedRead,countDocumentFlow,withCountPurchasedCommand} from '../../inventory/public/count-purchases';
import {requireTransferPurchasedRead,transferDocumentFlow,withTransferPurchasedCommand} from '../../inventory/public/transfer-purchases';
function flowAfterEffects(db: SqlDatabase, actor: StaffActor, key: string, kind: FlowKind, id: string): SqlStatement[] {
    const stateColumn = ["leave", "staff_task", "request", "order"].includes(kind) ? "state" : "status";
    const approvalIndex = `(SELECT current_step FROM approval_instances WHERE company_id=d.company_id AND record_id=d.id ORDER BY insertion_order DESC LIMIT 1)`;
    const state = kind === "expense" ? `CASE WHEN d.workflow_stage='department_approval' AND EXISTS(SELECT 1 FROM approval_steps s JOIN approval_instances i ON i.id=s.instance_id WHERE i.record_type='expense' AND i.record_id=d.id AND i.company_id=d.company_id AND s.ordinal=${approvalIndex} AND jsonb_exists(s.route_snapshot_json::jsonb,'departmentPolicyVersion')) THEN 'department_approval_' || (${approvalIndex})::text WHEN d.workflow_version=2 THEN d.workflow_stage ELSE CASE d.status WHEN 'in_review' THEN 'department_approval' WHEN 'approved' THEN 'gl_posting' WHEN 'posted' THEN 'payment' WHEN 'rejected' THEN 'draft' ELSE d.status END END` : kind === "leave" ? `CASE WHEN d.state='pending_approval' THEN 'approval_' || (${approvalIndex})::text ELSE d.state END` : kind === "staff_task" ? `CASE WHEN d.state IN ('ready','in_progress','rework') THEN 'work' WHEN d.state='start_approval' THEN 'start_' || (${approvalIndex})::text WHEN d.state='completion_approval' THEN 'completion_' || (${approvalIndex})::text ELSE d.state END` : kind === "request" ? "CASE WHEN d.state IN ('resolved','fulfilled') THEN 'confirm' WHEN d.state='returned' THEN 'draft' WHEN d.state='awaiting_approval' AND d.approval_json::jsonb #>> '{route,steps,0,snapshot,departmentPolicyVersion}' IS NOT NULL THEN 'purchase_' || COALESCE((d.approval_json::jsonb ->> 'currentStep')::int,0)::text WHEN d.state='awaiting_approval' AND d.approval_json::jsonb #>> '{route,steps,0,snapshot,spendingPolicy}' IS NOT NULL THEN 'purchase_' || (d.approval_json::jsonb #> '{route,steps}' -> COALESCE((d.approval_json::jsonb ->> 'currentStep')::int,0) ->> 'stage') ELSE d.state END" : kind === "invoice" ? "CASE WHEN d.status='partially_paid' THEN 'posted' ELSE d.status END" : kind === "order" ? `CASE WHEN d.state='partially_fulfilled' THEN 'confirmed' WHEN d.state='fulfilled' AND EXISTS(SELECT 1 ${commerceFinalSources.flow_routing_commerce_fulfillments_f_from} ${financeFinalSources.flow_routing_finance_journals_j_join} ON j.id=f.journal_id WHERE f.order_id=d.id AND j.status IN ('draft','in_review')) THEN 'stock_gl' WHEN d.state='fulfilled' AND EXISTS(SELECT 1 ${commerceFinalSources.flow_routing_commerce_fulfillments_f_from} WHERE f.order_id=d.id AND f.invoice_id IS NULL) THEN 'invoicing' ELSE d.state END` : kind === "payroll" ? `CASE WHEN d.status='posted' THEN COALESCE((SELECT CASE status WHEN 'draft' THEN 'payment_draft' WHEN 'approved' THEN 'payment_approved' WHEN 'exported' THEN 'payment_exported' WHEN 'partially_settled' THEN 'payment_partially_settled' WHEN 'settled' THEN 'payment_settled' ELSE 'posted' END ${peoplepayrollFinalSources.flow_routing_hr_payment_batches_from} WHERE company_id=d.company_id AND source_type='payroll' AND source_id=d.id AND status<>'cancelled' ORDER BY created_at DESC,id DESC LIMIT 1),'posted') ELSE d.status END` : `d.${stateColumn}`;
    const recipient = kind === "request" ? `CASE WHEN d.state='in_progress' THEN d.assignee_user_id ELSE r.steps_json::jsonb -> (${state}) ->> 'primaryUserId' END` : `r.steps_json::jsonb -> (${state}) ->> 'primaryUserId'`;
    const after = [stageWorkflowNotification(db, { id: crypto.randomUUID(), kind, now: Date.now(), companyId: actor.companyId, recordId: id, actorId: actor.userId, documentTable: tables[kind], recipientSql: recipient })];
    after.push(db.prepare(`UPDATE flow_events e SET next_step_key=(${state}),next_user_id=(${recipient}) FROM ${tables[kind]} d JOIN flow_routes r ON r.company_id=d.company_id AND r.kind=? AND r.record_id=d.id WHERE e.command_id=? AND d.company_id=? AND d.id=?`).bind(kind, key, actor.companyId, id));
    if (kind === 'journal')
        after.unshift(stageWorkflowPostedOrderRevision(db, Date.now(), id, actor.companyId));
    if (kind === 'journal')
        after.push(stageWorkflowInvoicingNotification(db, crypto.randomUUID(), Date.now(), id));
    return after;
}
const tables: Record<FlowKind, string> = { expense: "finance_expenses", leave: "leave_requests", staff_task: "staff_tasks", request: "employee_requests", stock_count: "inventory_counts", transfer: "inventory_transfers", order: "commerce_orders", invoice: "finance_invoices", journal: "finance_journals", payroll: "hr_payroll_runs", record: "workspace_records" };
function application(provided?: SqlDatabase) {
    let database = provided!;
    const repository: FlowRoutingRepository<SqlStatement> = { async readFlowRecordRow(companyId: string, id: string, kind: FlowKind) { const row = await workflowRecord(database, kind, companyId, id); if (row && kind === "order") return { ...row, has_service_lines: await commerce_flow_routing(database).readServiceOrder(companyId,id) ? 1 : 0 }; return row; },
        readWorkFulfillmentsRows(companyId: string) { return commerce_flow_routing(database).readWorkFulfillmentsRows(companyId); },
        readDefinitionsInstance(company_id: string, approvalKind: "expense" | "leave" | "request" | "stock_count" | "transfer" | "order" | "invoice" | "journal" | "payroll" | "record" | "task", id: string | number | null) { return database.prepare("SELECT * FROM approval_instances WHERE company_id=? AND record_type=? AND record_id=? ORDER BY insertion_order DESC LIMIT 1").bind(company_id, approvalKind, id).first<ApprovalInstance>(); },
        readDefinitionsApprovalSteps(id: string) { return database.prepare("SELECT * FROM approval_steps WHERE instance_id=? ORDER BY ordinal").bind(id).all<ApprovalStep>(); },
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
        readDefinitionsPaymentBatch(company_id: string, id: string | number | null) { if (!hasPolicyReadSnapshot()) return people_payroll_flow_routing(database).readDefinitionsPaymentBatch(company_id, id);
            return nativePolicyRead(database, JSON.stringify(["flow-payment-batches",company_id]), async () => new Map((await people_payroll_flow_routing(database).readWorkflowPaymentBatches(company_id)).results.map(row => [row.source_id,row]))).then(rows => rows.get(String(id)) ?? null); },
        readSharedFlowReferencesRoutesdefaultsevents(companyId: string) { return database.prepare("SELECT * FROM flow_routes WHERE company_id=?").bind(companyId).all<Route & {
            kind: string;
            record_id: string;
        }>(); },
        readSharedFlowReferencesRoutesdefaultsevents2(companyId: string) { return database.prepare("SELECT * FROM flow_defaults WHERE company_id=? ORDER BY CASE WHEN department_id IS NOT NULL THEN 3 WHEN branch_id IS NOT NULL THEN 2 ELSE 1 END DESC,revision DESC").bind(companyId).all<DefaultRow & {
            kind: string;
            branch_id: string | null;
            department_id: string | null;
        }>(); },
        readSharedFlowReferencesRoutesdefaultsevents3(companyId: string) { return database.prepare("SELECT kind,record_id,action,actor_name,created_at,reason,step_key FROM flow_events WHERE company_id=? ORDER BY created_at,id").bind(companyId).all<FlowHistoryRow>(); },
        readReadFlowRoute(companyId: string, kind: "expense" | "leave" | "staff_task" | "request" | "stock_count" | "transfer" | "order" | "invoice" | "journal" | "payroll" | "record", id: string) { return database.prepare("SELECT * FROM flow_routes WHERE company_id=? AND kind=? AND record_id=?").bind(companyId, kind, id).first<Route>(); },
        readReadFlowDefaultsRevision(companyId: string) { return database.prepare("SELECT COALESCE(MAX(revision),0) revision FROM flow_defaults WHERE company_id=?").bind(companyId).first<{
            revision: number;
        }>(); },
        readReadFlowDefaults(companyId: string, kind: "expense" | "leave" | "staff_task" | "request" | "stock_count" | "transfer" | "order" | "invoice" | "journal" | "payroll" | "record", branch_id: string | null, department_id: string | null, defaultsRevision: number) { return database.prepare(`SELECT revision,step_key,primary_user_id FROM flow_defaults WHERE company_id=? AND kind=? AND (branch_id IS NULL OR branch_id=?) AND (department_id IS NULL OR department_id=?) AND revision<=? ORDER BY CASE WHEN department_id IS NOT NULL THEN 3 WHEN branch_id IS NOT NULL THEN 2 ELSE 1 END DESC,revision DESC`).bind(companyId, kind, branch_id, department_id, defaultsRevision).all<DefaultRow>(); },
        readReadFlowPeople(companyId: string, now: number, now2: number) { return people_payroll_flow_routing(database).readReadFlowPeople(companyId, now, now2); },
        readReadFlowMembers(companyId: string, now: number, now2: number) { return identity_access_flow_routing(database).readReadFlowMembers(companyId, now, now2); },
        readReadFlowHistoryRows(companyId: string, kind: "expense" | "leave" | "staff_task" | "request" | "stock_count" | "transfer" | "order" | "invoice" | "journal" | "payroll" | "record", id: string) { return database.prepare("SELECT action,actor_name,created_at,reason,step_key FROM flow_events WHERE company_id=? AND kind=? AND record_id=? ORDER BY created_at,id").bind(companyId, kind, id).all<{
            action: string;
            actor_name: string;
            created_at: number;
            reason: string;
            step_key: string;
        }>(); },
        guard24(key: string, value: string, args: string[], version: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM flow_routes WHERE company_id=? AND kind=? AND record_id=? AND version=?)", [...args, version]); },
        guard25(key: string, value: string, companyId: string, revision: number) { return assertion(database, key, value, "(SELECT COALESCE(MAX(revision),0) FROM flow_defaults WHERE company_id=?)=?", [companyId, revision]); },
        stageRouteStatementsEffect(args: string[], revision: number, value: string, now: number) { return database.prepare("INSERT INTO flow_routes(company_id,kind,record_id,defaults_revision,steps_json,created_at) VALUES(?,?,?,?,?,?)").bind(...args, revision, value, now); },
        readPrepareFlowCommandPerson(companyId: string, userId: string) { return people_payroll_flow_routing(database).readPrepareFlowCommandPerson(companyId, userId); },
        stagePrepareFlowCommandSteps(value: string, key: string, companyId: string, kind: "expense" | "leave" | "staff_task" | "request" | "stock_count" | "transfer" | "order" | "invoice" | "journal" | "payroll" | "record", id: string, value2: string, action: string, userId: string, value3: string, reason: string, version: number, now: number) { return database.prepare("INSERT INTO flow_events(id,command_id,company_id,kind,record_id,step_key,action,actor_user_id,actor_name,reason,record_version,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(value, key, companyId, kind, id, value2, action, userId, value3, reason, version, now); },
        readPrepareFlowCommandParent(id: string, companyId: string) { return commerce_flow_routing(database).readPrepareFlowCommandParent(id, companyId); },
        readAssignFlowPrimaryExisting(companyId: string, kind: "expense" | "leave" | "staff_task" | "request" | "stock_count" | "transfer" | "order" | "invoice" | "journal" | "payroll" | "record", id: string) { return database.prepare("SELECT steps_json FROM flow_routes WHERE company_id=? AND kind=? AND record_id=?").bind(companyId, kind, id).first<{
            steps_json: string;
        }>(); },
        readAssignFlowPrimaryParent(id: string, companyId: string) { return commerce_flow_routing(database).readAssignFlowPrimaryParent(id, companyId); },
        stageAssignFlowPrimaryEffect(userId: string, companyId: string, order_id: string) { return database.prepare("UPDATE flow_routes SET steps_json=jsonb_set(steps_json::jsonb,'{stock_gl,primaryUserId}',to_jsonb(?::text))::text,version=version+1 WHERE company_id=? AND kind='order' AND record_id=?").bind(userId, companyId, order_id); },
        readAssignFlowPrimaryAdministrator(companyId: string, userId: string) { return people_payroll_flow_routing(database).readAssignFlowPrimaryAdministrator(companyId, userId); },
        guard34(key: string, value: "flow-document-version", companyId: string, id: string, recordVersion: number, kind: FlowKind) { return assertion(database, key, value, `EXISTS(SELECT 1 FROM ${tables[kind]} WHERE company_id=? AND id=? AND version=?)`, [companyId, id, recordVersion]); },
        stageAssignFlowPrimaryEffect2(value: string, companyId: string, kind: "expense" | "leave" | "staff_task" | "request" | "stock_count" | "transfer" | "order" | "invoice" | "journal" | "payroll" | "record", id: string) { return database.prepare("UPDATE flow_routes SET steps_json=?,version=version+1 WHERE company_id=? AND kind=? AND record_id=?").bind(value, companyId, kind, id); },
        stageAssignFlowPrimaryEffect3(companyId: string, kind: "expense" | "leave" | "staff_task" | "request" | "stock_count" | "transfer" | "order" | "invoice" | "journal" | "payroll" | "record", key: string, branch_id: string | null, department_id: string | null, userId: string, userId2: string, reason: string, now: number) { return database.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,branch_id,department_id,primary_user_id,created_by,reason,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(companyId, kind, key, branch_id, department_id, userId, userId2, reason, now); },
        stageAssignFlowPrimaryEffect4(value: string, key: string, companyId: string, kind: "expense" | "leave" | "staff_task" | "request" | "stock_count" | "transfer" | "order" | "invoice" | "journal" | "payroll" | "record", id: string, key2: string, userId: string, value2: string, value3: string, recordVersion: number, now: number) { return database.prepare("INSERT INTO flow_events(id,command_id,company_id,kind,record_id,step_key,action,actor_user_id,actor_name,reason,record_version,created_at) VALUES(?,?,?,?,?,?,'assign',?,?,?,?,?)").bind(value, key, companyId, kind, id, key2, userId, value2, value3, recordVersion, now); },
    };
    return createFlowRoutingApplication({ repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, commands: { replay: (...args) => nativeReplayCommand(database, ...args), commit: (...args) => nativeExecuteCommand(database, ...args) }, prepare: async (holding) => { if (!provided)
            database = (await nativeHoldingResources(holding)).write; }, production: process.env.NODE_ENV === 'production', batch: effects => database.batch(effects), policyRead: (key, read) => nativePolicyRead(database, key, read), policyNow: nativePolicyNow, flowAfterEffects: (...args) => flowAfterEffects(database, ...args), approvalView: (...args) => fromNative(() => approvalView(database, ...args)), canOverrideEmployeeRequest: (...args) => fromNative(() => canOverrideEmployeeRequest(database, ...args)), readRequestForOverride: (...args) => fromNative(() => readRequestForOverride(database, ...args)), assertProductionModuleEnabled: (...args) => fromNative(() => assertProductionModuleEnabled(database, ...args)), productionFeatureGuard: (...args) => productionFeatureGuard(database, ...args), hasPolicyReadSnapshot: (...args) => fromNativeSync(() => hasPolicyReadSnapshot(...args)), currentPersonPrincipals: (...args) => fromNative(() => currentPersonPrincipals(database, ...args)), personIdentityIds: (...args) => fromNative(() => personIdentityIds(database, ...args)), isTransactionalSqlDatabase: (...args) => isTransactionalSqlDatabase(database, ...args), companyAuthority: (...args) => fromNative(() => companyAuthority(database, ...args)), authorityGuard: (...args) => authorityGuard(database, ...args), notification: (...args) => notification(database, ...args), expenseRoleUsers: (...args) => fromNative(() => expenseRoleUsers(database, ...args)), expenseRoute: (...args) => fromNative(() => expenseRoute(database, ...args)), expensePoolCandidates: (...args) => fromNative(() => expensePoolCandidates(database, ...args)), expenseAuthoritySnapshot: (...args) => fromNative(() => expenseAuthoritySnapshot(database, ...args)), spendingStepCandidates: (...args) => fromNative(() => spendingStepCandidates(database, ...args)), spendingUsers: (...args) => fromNative(() => spendingUsers(database, ...args)), resolveServiceTeam: (...args) => fromNative(() => resolveServiceTeam(database, ...args)), teamHandlers: (...args) => fromNative(() => teamHandlers(database, ...args)), peopleRequestRoute: (...args) => fromNative(() => peopleRequestRoute(database, ...args)) });
}
async function fromNative<Result>(run: () => Promise<Result> | Result): Promise<Result> { try {
    return await run();
}
catch (error) {
    if (error instanceof NativeApiError)
        throw new BusinessRuleError(error.status, error.code, error.message);
    throw error;
} }
async function invoke<Result>(run: () => Promise<Result>) { try {
    return await run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new NativeApiError(error.status, error.code, error.message);
    throw error;
} }
function invokeSync<Result>(run: () => Result) { try {
    return run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new NativeApiError(error.status, error.code, error.message);
    throw error;
} }
export async function flowRecord(db: SqlDatabase, companyId: string, kind: FlowKind, id: string): Promise<Row> { return invoke(() => application(db).flowRecord(companyId, kind, id)); }
export function flowRoleAllowed(authority: string | PermissionActor, step: FlowStep | null) { return invokeSync(() => pureflowRoleAllowed(authority, step)); }
export async function resolveFlow(db: SqlDatabase, actor: StaffActor, kind: FlowKind, id: string): Promise<FlowView> {
    if(kind === "expense")await requireExpensePurchasedHistory(db,actor);
    if(kind === "leave")await requireWorkforceFlowHistory(db,actor,id);
    if(kind === "order")await requireCommerceFlowHistory(db,actor,id);
    if(kind === "stock_count")await requireCountPurchasedRead(db,actor,id);
    if(kind === "transfer")await requireTransferPurchasedRead(db,actor,{kind:"transfer",id});
    const view = await invoke(() => application(db).resolveFlow(actor, kind, id));
    return kind === "expense" ? expenseDocumentFlow(db,actor,view) : kind === "payroll" ? payrollDocumentFlow(db, actor, view) : kind === "order" ? commerceDocumentFlow(db,actor,view) : kind === "stock_count" ? countDocumentFlow(db,actor,view) : kind === "transfer" ? transferDocumentFlow(db,actor,view) : kind === "leave" ? workforceDocumentFlow(db,actor,view) : view;
}
export async function prepareFlowCommand(db: SqlDatabase, actor: StaffActor, key: string, kind: FlowKind, id: string, action: string, input: unknown, version: number): Promise<{
    before: SqlStatement[];
    after: SqlStatement[];
    view: FlowView;
}> { return invoke(() => application(db).prepareFlowCommand(actor, key, kind, id, action, input, version)); }
export async function assignFlowPrimary(db: SqlDatabase, actor: StaffActor, kind: FlowKind, id: string, input: {
    stepKey: string;
    userId: string;
    expectedVersion: number;
    recordVersion: number;
    reason: string;
    saveDefault: boolean;
}, key: string): Promise<Record<string, unknown>> {
    const run=(database:SqlDatabase)=>invoke(() => application(database).assignFlowPrimary(actor, kind, id, input, key));
    return kind === "expense" ? withExpenseFlowAssignment(db,actor,id,{key,stepKey:input.stepKey,saveDefault:input.saveDefault},run) : kind === "payroll" ? withPayrollPurchasedCommand(actor, {key,operation:"flow.assign",purpose:"settle-existing"}, tx => run(tx ?? db), db)
        : kind === "order" ? withCommerceFlowAssignment(db,actor,id,{key,stepKey:input.stepKey,saveDefault:input.saveDefault},run)
        : kind === "stock_count" ? withCountPurchasedCommand(db,actor,{key,operation:"flow.assign",id,purpose:"settle-existing",permission:"inventory.read",existing:true,flowStep:input.stepKey,saveDefault:input.saveDefault},run)
        : kind === "leave" ? withWorkforceFlowAssignment(db,actor,id,{key,stepKey:input.stepKey,saveDefault:input.saveDefault},run)
        : kind === "transfer" ? withTransferPurchasedCommand(db,actor,{key,operation:"flow.assign",target:{kind:"transfer",id},purpose:"settle-existing",permissions:[],existing:true,flowStep:input.stepKey,saveDefault:input.saveDefault},run) : run(db);
}
export function flowCommandTarget(operation: string, id: string, input: unknown): {
    kind: FlowKind;
    id: string;
    action: string;
} | null { return invokeSync(() => pureflowCommandTarget(operation, id, input)); }
function fromNativeSync<Result>(run: () => Result): Result { try {
    return run();
}
catch (error) {
    if (error instanceof NativeApiError)
        throw new BusinessRuleError(error.status, error.code, error.message);
    throw error;
} }
