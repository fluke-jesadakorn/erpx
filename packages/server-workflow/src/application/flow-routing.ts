import type { FlowKind, FlowPerson, FlowStep, FlowView } from "@erpx/feature-shared/lib/flow";
import type { StaffActor } from "@erpx/server-operations/public/staff-policy";
import type { ApprovalInstance, ApprovalStep } from "@erpx/server-workflow/public/approvals";
import type { SavedApproval } from "@erpx/server-records/public/employee-requests";
import type { approvalView } from "@erpx/server-workflow/public/approvals";
import type { canOverrideEmployeeRequest } from "@erpx/server-records/public/employee-requests";
import type { readRequestForOverride } from "@erpx/server-records/public/employee-requests";
import type { assertProductionModuleEnabled } from "@erpx/server-pms/public/hospitality-rollout";
import type { productionFeatureGuard } from "@erpx/server-pms/public/hospitality-rollout";
import type { hasPolicyReadSnapshot } from "@erpx/server-identity_access/public/iam-read-snapshot";
import type { currentPersonPrincipals } from "@erpx/server-identity_access/public/iam-person";
import type { personIdentityIds } from "@erpx/server-identity_access/public/iam-person";
import type { companyAuthority } from "@erpx/server-operations/public/staff-policy";
import type { authorityGuard } from "@erpx/server-operations/public/staff-policy";
import type { notification } from "@erpx/server-operations/public/staff-policy";
import type { expenseRoleUsers } from "@erpx/server-people_payroll/public/expense-routing";
import type { expenseRoute } from "@erpx/server-people_payroll/public/expense-routing";
import type { expensePoolCandidates } from "@erpx/server-people_payroll/public/expense-routing";
import type { expenseAuthoritySnapshot } from "@erpx/server-people_payroll/public/expense-routing";
import type { spendingStepCandidates } from "@erpx/server-people_payroll/public/expense-routing";
import type { spendingUsers } from "@erpx/server-people_payroll/public/expense-routing";
import type { resolveServiceTeam } from "@erpx/server-support/public/service-teams";
import type { teamHandlers } from "@erpx/server-support/public/service-teams";
import type { peopleRequestRoute } from "@erpx/server-organization/public/department-authority";
import { type Row, type Definition, type Route, type SavedStep, type DefaultRow, str, scopeFor, json, flowRoleAllowed, type FlowHistoryRow, flowCommandTarget } from '@erpx/server-workflow/domain/flow-routing';
import { BusinessRuleError, type Clock, type IdentifierSource } from '@erpx/server-shared/index';
import type { OwnerCommands } from '@erpx/server-workflow/domain/owner-command-ports';
type Tail<Value extends unknown[]> = Value extends [
    unknown,
    ...infer Rest
] ? Rest : never;
type PortValue<Value, Effect> = Value extends {
    bind(...args: never[]): unknown;
    run(): unknown;
} ? Effect : Value extends {
    guard: (...args: infer Args) => unknown;
} ? Omit<Value, 'guard'> & {
    guard(...args: Args): Effect;
} : Value extends readonly {
    bind(...args: never[]): unknown;
    run(): unknown;
}[] ? Effect[] : Value;
type PortResult<Value, Effect> = Value extends Promise<infer Result> ? Promise<PortValue<Result, Effect>> : PortValue<Value, Effect>;
export interface FlowRoutingRepository<Effect> {
    readFlowRecordRow(companyId: string, id: string, kind: "expense" | "leave" | "staff_task" | "request" | "stock_count" | "transfer" | "order" | "invoice" | "journal" | "payroll" | "record"): Promise<Row | null>;
    readWorkFulfillmentsRows(companyId: string): Promise<{
        results: ({
            id: string;
            order_id: string;
            invoice_id: string | null;
            journal_status: string | null;
            ready: boolean;
        })[];
    }>;
    readDefinitionsInstance(company_id: string, approvalKind: "expense" | "leave" | "request" | "stock_count" | "transfer" | "order" | "invoice" | "journal" | "payroll" | "record" | "task", id: string | number | null): Promise<ApprovalInstance | null>;
    readDefinitionsApprovalSteps(id: string): Promise<{
        results: (ApprovalStep)[];
    }>;
    readDefinitionsJournal(journal_id: string | number, company_id: string): Promise<{
        posted_by: string;
    } | null>;
    readDefinitionsDelegation(company_id: string, id: string | number | null): Promise<{
        delegate_user_id: string;
    } | null>;
    readDefinitionsTeam(company_id: string, teamId: string): Promise<{
        name: string;
    } | null>;
    readDefinitionsPending(id: string | number | null): Promise<Record<string, unknown> | null>;
    readDefinitionsReady(id: string | number | null): Promise<Record<string, unknown> | null>;
    readDefinitionsStockGl(id: string | number | null): Promise<Record<string, unknown> | null>;
    readDefinitionsRequest(source_id: string | number | null, company_id: string): Promise<{
        requester_user_id: string;
        created_by: string;
        approval_json: string;
    } | null>;
    readDefinitionsJournal2(journal_id: string | number, company_id: string): Promise<{
        posted_by: string;
    } | null>;
    readDefinitionsDelivery(source_id: string | number | null): Promise<{
        order_id: string;
    } | null>;
    readDefinitionsPaymentBatch(company_id: string, id: string | number | null): Promise<{
        id: string;
        status: string;
        created_by: string;
    } | null>;
    readSharedFlowReferencesRoutesdefaultsevents(companyId: string): Promise<{
        results: (Route & {
            kind: string;
            record_id: string;
        })[];
    }>;
    readSharedFlowReferencesRoutesdefaultsevents2(companyId: string): Promise<{
        results: (DefaultRow & {
            kind: string;
            branch_id: string | null;
            department_id: string | null;
        })[];
    }>;
    readSharedFlowReferencesRoutesdefaultsevents3(companyId: string): Promise<{
        results: (FlowHistoryRow)[];
    }>;
    readReadFlowRoute(companyId: string, kind: "expense" | "leave" | "staff_task" | "request" | "stock_count" | "transfer" | "order" | "invoice" | "journal" | "payroll" | "record", id: string): Promise<Route | null>;
    readReadFlowDefaultsRevision(companyId: string): Promise<{
        revision: number;
    } | null>;
    readReadFlowDefaults(companyId: string, kind: "expense" | "leave" | "staff_task" | "request" | "stock_count" | "transfer" | "order" | "invoice" | "journal" | "payroll" | "record", branch_id: string | null, department_id: string | null, defaultsRevision: number): Promise<{
        results: (DefaultRow)[];
    }>;
    readReadFlowPeople(companyId: string, now: number, now2: number): Promise<{
        results: ({
            user_id: string;
            legal_name: string;
            department_id: string | null;
            team_name: string | null;
        })[];
    }>;
    readReadFlowMembers(companyId: string, now: number, now2: number): Promise<{
        results: ({
            user_id: string;
            name: string;
        })[];
    }>;
    readReadFlowHistoryRows(companyId: string, kind: "expense" | "leave" | "staff_task" | "request" | "stock_count" | "transfer" | "order" | "invoice" | "journal" | "payroll" | "record", id: string): Promise<{
        results: ({
            action: string;
            actor_name: string;
            created_at: number;
            reason: string;
            step_key: string;
        })[];
    }>;
    guard24(key: string, value: string, args: string[], version: number): Effect;
    guard25(key: string, value: string, companyId: string, revision: number): Effect;
    stageRouteStatementsEffect(args: string[], revision: number, value: string, now: number): Effect;
    readPrepareFlowCommandPerson(companyId: string, userId: string): Promise<{
        legal_name: string;
    } | null>;
    stagePrepareFlowCommandSteps(value: string, key: string, companyId: string, kind: "expense" | "leave" | "staff_task" | "request" | "stock_count" | "transfer" | "order" | "invoice" | "journal" | "payroll" | "record", id: string, value2: string, action: string, userId: string, value3: string, reason: string, version: number, now: number): Effect;
    readPrepareFlowCommandParent(id: string, companyId: string): Promise<{
        order_id: string;
    } | null>;
    readAssignFlowPrimaryExisting(companyId: string, kind: "expense" | "leave" | "staff_task" | "request" | "stock_count" | "transfer" | "order" | "invoice" | "journal" | "payroll" | "record", id: string): Promise<{
        steps_json: string;
    } | null>;
    readAssignFlowPrimaryParent(id: string, companyId: string): Promise<{
        order_id: string;
    } | null>;
    stageAssignFlowPrimaryEffect(userId: string, companyId: string, order_id: string): Effect;
    readAssignFlowPrimaryAdministrator(companyId: string, userId: string): Promise<{
        legal_name: string;
    } | null>;
    guard34(key: string, value: "flow-document-version", companyId: string, id: string, recordVersion: number, kind: FlowKind): Effect;
    stageAssignFlowPrimaryEffect2(value: string, companyId: string, kind: "expense" | "leave" | "staff_task" | "request" | "stock_count" | "transfer" | "order" | "invoice" | "journal" | "payroll" | "record", id: string): Effect;
    stageAssignFlowPrimaryEffect3(companyId: string, kind: "expense" | "leave" | "staff_task" | "request" | "stock_count" | "transfer" | "order" | "invoice" | "journal" | "payroll" | "record", key: string, branch_id: string | null, department_id: string | null, userId: string, userId2: string, reason: string, now: number): Effect;
    stageAssignFlowPrimaryEffect4(value: string, key: string, companyId: string, kind: "expense" | "leave" | "staff_task" | "request" | "stock_count" | "transfer" | "order" | "invoice" | "journal" | "payroll" | "record", id: string, key2: string, userId: string, value2: string, value3: string, recordVersion: number, now: number): Effect;
}
export interface FlowRoutingPorts<Effect> {
    repository: FlowRoutingRepository<Effect>;
    clock: Clock;
    identifiers: IdentifierSource;
    commands: OwnerCommands<Effect>;
    prepare(holdingId: string): Promise<void>;
    production: boolean;
    batch(effects: Effect[]): Promise<unknown>;
    policyRead<Result>(key: string, read: () => Promise<Result>): Promise<Result>;
    policyNow(): number;
    flowAfterEffects(actor: StaffActor, key: string, kind: FlowKind, id: string): Effect[];
    approvalView(...args: Tail<Parameters<typeof approvalView>>): PortResult<ReturnType<typeof approvalView>, Effect>;
    canOverrideEmployeeRequest(...args: Tail<Parameters<typeof canOverrideEmployeeRequest>>): PortResult<ReturnType<typeof canOverrideEmployeeRequest>, Effect>;
    readRequestForOverride(...args: Tail<Parameters<typeof readRequestForOverride>>): PortResult<ReturnType<typeof readRequestForOverride>, Effect>;
    assertProductionModuleEnabled(...args: Tail<Parameters<typeof assertProductionModuleEnabled>>): PortResult<ReturnType<typeof assertProductionModuleEnabled>, Effect>;
    productionFeatureGuard(...args: Tail<Parameters<typeof productionFeatureGuard>>): PortResult<ReturnType<typeof productionFeatureGuard>, Effect>;
    hasPolicyReadSnapshot(...args: Parameters<typeof hasPolicyReadSnapshot>): PortResult<ReturnType<typeof hasPolicyReadSnapshot>, Effect>;
    currentPersonPrincipals(...args: Tail<Parameters<typeof currentPersonPrincipals>>): PortResult<ReturnType<typeof currentPersonPrincipals>, Effect>;
    personIdentityIds(...args: Tail<Parameters<typeof personIdentityIds>>): PortResult<ReturnType<typeof personIdentityIds>, Effect>;
    isTransactionalSqlDatabase(): boolean;
    companyAuthority(...args: Tail<Parameters<typeof companyAuthority>>): PortResult<ReturnType<typeof companyAuthority>, Effect>;
    authorityGuard(...args: Tail<Parameters<typeof authorityGuard>>): PortResult<ReturnType<typeof authorityGuard>, Effect>;
    notification(...args: Tail<Parameters<typeof notification>>): PortResult<ReturnType<typeof notification>, Effect>;
    expenseRoleUsers(...args: Tail<Parameters<typeof expenseRoleUsers>>): PortResult<ReturnType<typeof expenseRoleUsers>, Effect>;
    expenseRoute(...args: Tail<Parameters<typeof expenseRoute>>): PortResult<ReturnType<typeof expenseRoute>, Effect>;
    expensePoolCandidates(...args: Tail<Parameters<typeof expensePoolCandidates>>): PortResult<ReturnType<typeof expensePoolCandidates>, Effect>;
    expenseAuthoritySnapshot(...args: Tail<Parameters<typeof expenseAuthoritySnapshot>>): PortResult<ReturnType<typeof expenseAuthoritySnapshot>, Effect>;
    spendingStepCandidates(...args: Tail<Parameters<typeof spendingStepCandidates>>): PortResult<ReturnType<typeof spendingStepCandidates>, Effect>;
    spendingUsers(...args: Tail<Parameters<typeof spendingUsers>>): PortResult<ReturnType<typeof spendingUsers>, Effect>;
    resolveServiceTeam(...args: Tail<Parameters<typeof resolveServiceTeam>>): PortResult<ReturnType<typeof resolveServiceTeam>, Effect>;
    teamHandlers(...args: Tail<Parameters<typeof teamHandlers>>): PortResult<ReturnType<typeof teamHandlers>, Effect>;
    peopleRequestRoute(...args: Tail<Parameters<typeof peopleRequestRoute>>): PortResult<ReturnType<typeof peopleRequestRoute>, Effect>;
}
export function createFlowRoutingApplication<Effect>(ports: FlowRoutingPorts<Effect>) {
    const r = ports.repository;
    async function flowRecord(companyId: string, kind: FlowKind, id: string): Promise<Row> {
        const row = await ports.policyRead(JSON.stringify(["flow-record", companyId, kind, id]), () => r.readFlowRecordRow(companyId, id, kind));
        if (!row)
            throw new BusinessRuleError(404, "FLOW_NOT_FOUND", "Workflow document not found in this company.");
        return row;
    }
    async function workFulfillments(companyId: string) {
        return ports.policyRead(JSON.stringify(["work-fulfillments", companyId]), async () => {
            const rows = (await r.readWorkFulfillmentsRows(companyId)).results;
            const orders = new Map<string, {
                pending: boolean;
                ready: boolean;
                stockGl: boolean;
            }>();
            for (const row of rows) {
                const state = orders.get(row.order_id) ?? { pending: false, ready: false, stockGl: false };
                state.pending ||= row.invoice_id === null;
                state.ready ||= row.invoice_id === null && row.ready;
                state.stockGl ||= ["draft", "in_review"].includes(row.journal_status ?? "");
                orders.set(row.order_id, state);
            }
            return { orders, byId: new Map(rows.map(row => [row.id, row])) };
        });
    }
    async function definitions(kind: FlowKind, row: Row, actor: StaffActor) {
        const scope = scopeFor(row), owner = str(row.requester_user_id || row.claimant_user_id || row.creator_user_id || row.created_by);
        const draftOwner = str(row.created_by || row.creator_user_id) || owner;
        const excluded = new Set([owner, str(row.created_by), str(row.prepared_by)]);
        const candidateRoles = new WeakMap<string[], string[]>();
        const roles = async (names: string[], independent = false, target = scope) => {
            // A transaction database holds one client; only the pooled database may
            // expand role sources concurrently. Keep command reads ordered on that client.
            const sources: string[][] = [];
            if (ports.isTransactionalSqlDatabase())
                sources.push(...await Promise.all(names.map(role => ports.expenseRoleUsers(target, role))));
            else
                for (const role of names)
                    sources.push(await ports.expenseRoleUsers(target, role));
            const ids = [...new Set(sources.flat())].filter(id => !independent || !excluded.has(id));
            candidateRoles.set(ids, names);
            return ids;
        };
        const steps: Definition[] = [];
        const add = (key: string, action: string, role: string, ids: string[], preferred?: string, blocker?: string) => { steps.push({ key, action, role, ids, preferred, blocker, allowedRoles: candidateRoles.get(ids) ?? (key === "draft" && ["stock_count", "transfer"].includes(kind) ? ["warehouse_manager", "operations_manager", "holding_owner"] : key === "draft" && kind === "order" ? [role, role === "purchasing_operator" ? "purchasing_manager" : "sales_manager", "holding_owner"] : ["requester", "assignee", "team_lead", "team_member", "personal_approver"].includes(role) ? [] : [["named", "department"].includes(role) ? "people_manager" : role]) }); };
        let current = str(row.workflow_stage || row.state || row.status), terminal = false;
        if (row.cancelled_at || ["cancelled", "completed", "closed", "reimbursed", "reversed", "void"].includes(current))
            terminal = true;
        const approvalKind = kind === "staff_task" ? "task" : kind;
        const instance = ["expense", "leave", "staff_task"].includes(kind) ? await r.readDefinitionsInstance(scope.company_id, approvalKind, row.id) : null;
        const approvalSteps = instance ? (await r.readDefinitionsApprovalSteps(instance.id)).results : [];
        const decisions = approvalSteps.filter(s => s.state === "approved").map(s => s.approver_user_id);
        if (kind === "expense") {
            if (row.workflow_version !== 2)
                current = ({ draft: "draft", rejected: "draft", in_review: "department_approval", approved: "gl_posting", posted: "payment", reimbursed: "completed" } as Record<string, string>)[str(row.status)] ?? current;
            add("draft", "Prepare and submit claim", "requester", [draftOwner], draftOwner);
            const fresh = approvalSteps.length ? null : await ports.expenseRoute(scope.company_id, owner, [str(row.prepared_by || row.created_by)], false, row.request_kind === "payment" ? "payment" : "expense");
            const policyDepartments = approvalSteps.filter(s => s.stage === "department_approval" && json<{
                departmentPolicyVersion?: number;
            }>(s.route_snapshot_json ?? null, {}).departmentPolicyVersion !== undefined);
            const proposedDepartments = fresh?.steps.filter(s => s.stage === "department_approval" && s.snapshot?.departmentPolicyVersion !== undefined) ?? [];
            if (policyDepartments.length || proposedDepartments.length) {
                for (const [index, s] of (policyDepartments.length ? policyDepartments : proposedDepartments).entries()) {
                    const saved = "ordinal" in s ? s as ApprovalStep : null;
                    const user = saved?.approver_user_id ?? ("userId" in s ? s.userId : "");
                    add(`department_approval_${index}`, `Department approval · step ${index + 1}`, "personal_approver", user ? [user] : [], user || undefined);
                }
                if (current === "department_approval")
                    current = `department_approval_${instance?.current_step ?? 0}`;
            }
            for (const [index, key, label, role] of [[0, "department_approval", "Approve business purpose", "people_manager"], [1, "accounts_review", "Prepare and check Debit / Credit", "finance_preparer"], [2, "accounts_approval", "Approve accounting preparation", "finance_approver"]] as const) {
                if (key === "department_approval" && (policyDepartments.length || proposedDepartments.length))
                    continue;
                const saved = approvalSteps.find(s => s.stage === key) ?? (approvalSteps.some(s => s.stage) ? undefined : approvalSteps[index]);
                const proposed = fresh?.steps.find(s => s.stage === key);
                if (key === "department_approval" && !saved && ((approvalSteps.length > 0) || (fresh && !fresh.blockedReason && !proposed)))
                    continue;
                let ids = saved ? saved.approver_pool_json && instance ? await ports.expensePoolCandidates(instance, saved, approvalSteps) : [saved.approver_user_id] : proposed?.candidateUserIds ?? (proposed ? [proposed.userId] : []);
                if (saved?.state === "approved")
                    ids = [saved.approver_user_id];
                add(key, label, role, ids, saved && !saved.approver_pool_json ? saved.approver_user_id : undefined, !ids.length ? fresh?.blockedReason ?? undefined : undefined);
            }
            const accounting = json<{
                preparedBy?: string;
            }>(row.accounting_json, {});
            const strict = approvalSteps.some(s => json<{
                spendingPolicy?: number;
            }>(s.route_snapshot_json ?? null, {}).spendingPolicy);
            const posters = (strict ? (await ports.spendingUsers(scope, "finance_approver")).map(p => p.user_id!).filter(id => !excluded.has(id)) : await roles(["finance_approver"], true)).filter(id => id !== accounting.preparedBy);
            add("gl_posting", "Review and post balanced GL", "finance_approver", posters, str(row.approved_by) || undefined);
            const payers = (strict ? (await ports.spendingUsers(scope, "treasury")).map(p => p.user_id!).filter(id => !excluded.has(id)) : await roles(["treasury"], true)).filter(id => id !== accounting.preparedBy && id !== row.approved_by && !decisions.includes(id));
            if (row.journal_id) {
                const journal = await r.readDefinitionsJournal(row.journal_id, scope.company_id);
                add("payment", "Record payment and settle payable", "treasury", payers.filter(id => id !== journal?.posted_by));
            }
            else
                add("payment", "Record payment and settle payable", "treasury", payers);
            if (row.status === "rejected")
                current = "draft";
        }
        else if (kind === "leave" || kind === "staff_task") {
            add("draft", "Prepare and submit", "requester", [draftOwner], draftOwner);
            if (kind === "leave") {
                const route = approvalSteps.length ? null : await ports.peopleRequestRoute(scope.company_id, owner);
                const list = approvalSteps.length ? approvalSteps : (route?.steps ?? []).map((s, ordinal) => ({ ordinal, authority: s.authority, approver_user_id: s.userId }));
                for (const s of list) {
                    const snapshot = "route_snapshot_json" in s ? json<{
                        departmentPolicyVersion?: number;
                        appointedPersonId?: string;
                    }>((s as ApprovalStep).route_snapshot_json ?? null, {}) : route?.steps[s.ordinal]?.snapshot;
                    add(`approval_${s.ordinal}`, s.authority === "hr_admin" ? "HR approval" : "Department approval", snapshot && (snapshot.departmentPolicyVersion !== undefined || snapshot.appointedPersonId) ? "personal_approver" : s.authority, [s.approver_user_id], s.approver_user_id);
                }
                current = current === "pending_approval" ? `approval_${instance?.current_step ?? 0}` : current === "rejected" ? "draft" : current;
                terminal ||= current === "approved";
            }
            else {
                const start = json<string[]>(row.start_approvers_json, []), complete = json<string[]>(row.completion_approvers_json, []);
                const assigned = approvalSteps.some(s => json<{
                    departmentPolicyVersion?: number;
                }>(s.route_snapshot_json ?? null, {}).departmentPolicyVersion !== undefined);
                const startSteps = assigned && instance?.phase === "start" ? approvalSteps.map(s => s.approver_user_id) : start;
                startSteps.forEach((id, i) => add(`start_${i}`, assigned && instance?.phase === "start" && approvalSteps[i]?.stage === "department_approval" ? "Department approval" : "Approve task start", "personal_approver", [id], id));
                const delegation = await r.readDefinitionsDelegation(scope.company_id, row.id);
                const workOwner = delegation?.delegate_user_id ?? str(row.assignee_user_id);
                add("work", "Start / complete assigned work", "assignee", [workOwner], workOwner);
                const completionSteps = assigned && instance?.phase === "completion" ? approvalSteps.map(s => s.approver_user_id) : complete;
                completionSteps.forEach((id, i) => add(`completion_${i}`, assigned && instance?.phase === "completion" && approvalSteps[i]?.stage === "department_approval" ? "Department approval" : "Approve task completion", "personal_approver", [id], id));
                current = current === "start_approval" ? `start_${instance?.current_step ?? 0}` : current === "completion_approval" ? `completion_${instance?.current_step ?? 0}` : ["ready", "in_progress", "rework"].includes(current) ? "work" : current;
                // Rerouted named approvals remain authoritative.
                for (const s of approvalSteps) {
                    const d = steps.find(d => d.key === `${instance?.phase}_${s.ordinal}`);
                    if (d) {
                        d.ids = [s.approver_user_id];
                        d.preferred = s.approver_user_id;
                    }
                }
            }
        }
        else if (kind === "request") {
            add("draft", "Prepare and submit request", "requester", [draftOwner], draftOwner);
            if (row.kind !== "service") {
                const approval = json<SavedApproval>(row.approval_json, {});
                const strict = approval.route?.steps.some(s => s.snapshot?.spendingPolicy || s.snapshot?.departmentPolicyVersion !== undefined);
                const policy = approval.route?.steps.some(s => s.snapshot?.departmentPolicyVersion !== undefined);
                const fresh = !approval.route && row.kind === "purchase" ? await ports.expenseRoute(scope.company_id, owner, [str(row.created_by)], false, "purchase") : null;
                if (strict || fresh) {
                    const route = approval.route ?? fresh!;
                    const labels: Record<string, string> = { department_approval: "Approve business purpose", accounts_review: "Prepare planned spending details", accounts_approval: "Approve planned spending before ordering" };
                    for (const [index, step] of route.steps.entries()) {
                        const decision = approval.decisions?.find(d => d.stage === step.stage);
                        const pool = decision ? [decision.userId] : await ports.spendingStepCandidates(scope, owner, [str(row.created_by)], step, (approval.decisions ?? []).filter(d => route.steps.findIndex(s => s.stage === d.stage) < index).map(d => d.userId));
                        add(policy ? `purchase_${index}` : `purchase_${step.stage}`, labels[step.stage!] ?? "Approve spending", step.snapshot?.departmentPolicyVersion !== undefined ? "personal_approver" : step.requiredRole ?? "people_manager", pool, decision?.userId ?? step.userId);
                    }
                    if (current === "awaiting_approval")
                        current = policy ? `purchase_${approval.currentStep ?? 0}` : `purchase_${route.steps[approval.currentStep ?? 0]?.stage}`;
                }
                else {
                    const route = await ports.expenseRoute(scope.company_id, owner, [str(row.created_by)], true);
                    add("awaiting_approval", "Approve department request", "people_manager", (route.steps[0]?.candidateUserIds ?? []).filter(id => !approval.pool || approval.pool.includes(id)));
                }
            }
            const resolved = row.team_id ? null : await ports.resolveServiceTeam(scope, str(row.category));
            const teamId = str(row.team_id) || resolved?.team.id || null;
            const handlers = await ports.teamHandlers(scope, teamId);
            const leads = handlers.filter(h => h.responsibility === "lead");
            add("queued", "Assign request to a team member", "team_lead", leads.map(h => h.user_id));
            add("in_progress", "Handle request and record outcome", "team_member", handlers.map(h => h.user_id), str(row.assignee_user_id) || undefined);
            if (teamId) {
                const team = await r.readDefinitionsTeam(scope.company_id, teamId);
                for (const s of steps.filter(s => ["queued", "in_progress"].includes(s.key)))
                    s.teamName = team?.name;
            }
            add("waiting_requester", "Reply with the requested information", "requester", [owner], owner);
            add("confirm", "Confirm the completed work", "requester", [owner], owner);
            current = ["returned", "draft"].includes(current) ? "draft" : ["resolved", "fulfilled"].includes(current) ? "confirm" : current;
            terminal ||= current === "rejected";
        }
        else if (kind === "stock_count" || kind === "transfer") {
            const target = kind === "transfer" ? { ...scope, branch_id: str(row.target_branch_id) || null, department_id: null } : { ...scope, branch_id: null, department_id: null };
            add("draft", "Prepare and submit warehouse document", "warehouse_manager", [owner], owner);
            add("submitted", kind === "transfer" ? row.delivery_mode === "transit" ? "Approve transport for source dispatch" : "Confirm receipt and post stock transfer" : "Approve stock count for GL review", kind === "transfer" ? "warehouse_manager" : "finance_approver", await roles(kind === "transfer" ? ["warehouse_manager", "operations_manager", "finance_approver", "holding_owner"] : ["finance_approver", "holding_owner"], true, target));
            if (kind === "stock_count")
                add("approved", "Review Debit / Credit and post stock adjustment", "finance_approver", await roles(["finance_approver", "holding_owner"], true, target), str(row.approved_by) || undefined);
            current = current === "rejected" ? "draft" : current;
            terminal ||= current === "posted";
        }
        else if (kind === "order") {
            const purchase = row.kind === "purchase";
            add("draft", "Prepare and submit order", purchase ? "purchasing_operator" : "sales_operator", [owner], owner);
            add("submitted", "Approve order", purchase ? "purchasing_manager" : "sales_manager", await roles(purchase ? ["purchasing_manager", "holding_owner"] : ["sales_manager", "holding_owner"], true));
            add("confirmed", row.kind === "quote" ? "Convert quotation to sales order" : purchase ? "Receive goods" : "Deliver goods / confirm completed services", row.kind === "quote" ? "sales_operator" : "warehouse_manager", await roles(row.kind === "quote" ? ["sales_operator", "sales_manager", "holding_owner"] : purchase || !row.has_service_lines ? ["warehouse_manager", "operations_manager", "holding_owner"] : ["sales_operator", "sales_manager", "warehouse_manager", "operations_manager", "holding_owner"]));
            add("stock_gl", "Review and post goods movement GL", "finance_approver", await roles(["finance_approver", "holding_owner"], true));
            add("invoicing", "Prepare invoice for independent GL review", "finance_preparer", await roles(["finance_preparer", "finance_approver", "holding_owner"]));
            const sharedFulfillments = ports.hasPolicyReadSnapshot() ? await workFulfillments(scope.company_id) : null;
            const state = sharedFulfillments?.orders.get(str(row.id));
            const pending = sharedFulfillments ? !!state?.pending : await r.readDefinitionsPending(row.id);
            if (purchase && pending) {
                const ready = sharedFulfillments ? !!state?.ready : await r.readDefinitionsReady(row.id);
                if (!ready) {
                    const step = steps.find(s => s.key === 'invoicing');
                    if (step)
                        step.blocker = 'Supplier invoice matching requires independent review or supplier correction before AP preparation.';
                }
            }
            const stockGl = sharedFulfillments ? !!state?.stockGl : await r.readDefinitionsStockGl(row.id);
            if (current === "fulfilled" && stockGl)
                current = "stock_gl";
            else if (current === "fulfilled" && pending)
                current = "invoicing";
            terminal ||= ["fulfilled", "converted"].includes(current);
            if (current === "partially_fulfilled")
                current = "confirmed";
        }
        else if (kind === "invoice") {
            const request = row.source_type === "commerce_fulfillment" ? await r.readDefinitionsRequest(row.source_id, scope.company_id) : null;
            const savedApproval = request ? JSON.parse(request.approval_json) as SavedApproval : null;
            const strict = savedApproval?.route?.steps.some(s => s.snapshot?.spendingPolicy);
            const excludedPayers = new Set([str(row.created_by), str(row.approved_by), request?.requester_user_id, request?.created_by, ...(savedApproval?.decisions ?? []).map(d => d.userId)]);
            if (row.journal_id) {
                const journal = await r.readDefinitionsJournal2(row.journal_id, scope.company_id);
                if (journal)
                    excludedPayers.add(journal.posted_by);
            }
            const accountants = strict ? (await ports.spendingUsers(scope, "finance_approver")).map(p => p.user_id!).filter(id => id !== row.created_by && id !== request?.requester_user_id && id !== request?.created_by) : await roles(["finance_approver", "holding_owner"], true);
            add("draft", "Review invoice Debit / Credit and post GL", "finance_approver", accountants);
            const payers = row.kind === "payable" ? (await ports.spendingUsers(scope, "treasury")).map(p => p.user_id!) : await roles(["treasury"], true);
            add("posted", row.kind === "receivable" ? "Record receipt and settle receivable" : "Record payment and attach bank slip", "treasury", payers.filter(id => !excludedPayers.has(id)));
            current = current === "in_review" ? "draft" : current === "partially_paid" ? "posted" : current;
            terminal ||= ["paid", "settled", "voided", "no_charge_confirmed"].includes(current) || (current === "posted" && Number(row.outstanding_minor) === 0);
        }
        else if (kind === "journal") {
            let sourcePrimary: string | undefined;
            if (["goods_receipt", "delivery"].includes(str(row.source_type))) {
                const delivery = (ports.hasPolicyReadSnapshot() ? (await workFulfillments(scope.company_id)).byId.get(str(row.source_id)) : null) ?? await r.readDefinitionsDelivery(row.source_id);
                if (delivery)
                    sourcePrimary = (await resolveFlow(actor, "order", delivery.order_id)).steps.find(s => s.key === "stock_gl")?.primary?.userId;
            }
            add("draft", "Review Debit / Credit and post GL", "finance_approver", await roles(["finance_approver", "holding_owner"], true), sourcePrimary);
            current = current === "in_review" ? "draft" : current;
            terminal ||= current === "posted";
        }
        else if (kind === "payroll") {
            const paymentBatch = await r.readDefinitionsPaymentBatch(scope.company_id, row.id);
            add("calculate", "Calculate payroll preview", "payroll_preparer", await roles(["payroll_preparer", "hr_admin"]));
            add("preview", "Check payroll and submit review", "payroll_preparer", await roles(["payroll_preparer", "hr_admin"]));
            const payrollApprovers = await roles(["payroll_approver", "people_manager"], true);
            if (row.submitted_by) {
                const submittedIdentities = new Set(await ports.personIdentityIds(scope.company_id, [String(row.submitted_by)]));
                for (let index = payrollApprovers.length - 1; index >= 0; index--)
                    if (submittedIdentities.has(payrollApprovers[index]))
                        payrollApprovers.splice(index, 1);
            }
            add("in_review", "Review and approve payroll", "payroll_approver", payrollApprovers);
            add("approved", "Post payroll GL and prepare payment", "finance_approver", await roles(["finance_approver", "holding_owner"], true));
            // Treasury is an eligible preparer too; company routing defaults may place
            // this step with Treasury before the independent Finance/Treasury review.
            add("posted", "Prepare payroll payment batch", "finance_approver", await roles(["finance_approver", "treasury"], true));
            const paymentApprovers = await roles(["finance_approver", "treasury"], true);
            // Keep the candidate array identity returned by roles(). The allowed-role
            // metadata is keyed by that array, so filtering in place preserves both
            // finance_approver and treasury as valid independent approvers.
            if (paymentBatch?.created_by) {
                for (let index = paymentApprovers.length - 1; index >= 0; index -= 1)
                    if (paymentApprovers[index] === paymentBatch.created_by)
                        paymentApprovers.splice(index, 1);
            }
            add("payment_draft", "Approve payroll payment batch", "finance_approver", paymentApprovers);
            add("payment_approved", "Export payroll payment instruction", "treasury", await roles(["treasury"], true));
            add("payment_exported", "Confirm payroll payment results", "treasury", await roles(["treasury"], true));
            add("payment_partially_settled", "Retry failed payroll payments", "treasury", await roles(["treasury"], true));
            add("payment_settled", "Payroll payment settled", "treasury", await roles(["treasury"], true));
            if (["draft", "calculating", "failed"].includes(current))
                current = "calculate";
            else if (current === "posted") {
                current = paymentBatch?.status === "draft" ? "payment_draft"
                    : paymentBatch?.status === "approved" ? "payment_approved"
                        : paymentBatch?.status === "exported" ? "payment_exported"
                            : paymentBatch?.status === "partially_settled" ? "payment_partially_settled"
                                : paymentBatch?.status === "settled" ? "payment_settled"
                                    : "posted";
            }
            terminal ||= paymentBatch?.status === "settled";
        }
        else {
            add("draft", "Prepare and submit record", "requester", [owner], owner);
            add("in_review", "Review and approve record", "people_manager", await roles(["people_manager", "holding_owner"], true));
            add("approved", "Complete approved work", "assignee", [owner], owner);
            if (current === "rejected")
                current = "draft";
        }
        return { scope, steps, current, terminal, instance, approvalSteps };
    }
    function sharedFlowReferences(companyId: string) {
        return ports.policyRead(JSON.stringify(["flow-references", companyId]), async () => {
            let readQueue: Promise<unknown> = Promise.resolve();
            const read = <T>(query: () => Promise<T>): Promise<T> => { if (ports.isTransactionalSqlDatabase())
                return query(); const result = readQueue.then(query); readQueue = result; return result; };
            const [routes, defaults, events] = await Promise.all([
                read(() => r.readSharedFlowReferencesRoutesdefaultsevents(companyId)),
                read(() => r.readSharedFlowReferencesRoutesdefaultsevents2(companyId)),
                read(() => r.readSharedFlowReferencesRoutesdefaultsevents3(companyId)),
            ]);
            const histories = new Map<string, FlowHistoryRow[]>();
            for (const event of events.results) {
                const key = `${event.kind}:${event.record_id}`;
                const rows = histories.get(key) ?? [];
                rows.push(event);
                histories.set(key, rows);
            }
            return { routes: new Map(routes.results.map(row => [`${row.kind}:${row.record_id}`, row])), defaults: defaults.results, histories,
                revision: defaults.results.reduce((max, row) => Math.max(max, row.revision), 0) };
        });
    }
    async function resolveFlow(actor: StaffActor, kind: FlowKind, id: string): Promise<FlowView> {
        return ports.policyRead(JSON.stringify(["flow-view", actor, kind, id]), () => readFlow(actor, kind, id));
    }
    async function readFlow(actor: StaffActor, kind: FlowKind, id: string): Promise<FlowView> {
        const row = await flowRecord(actor.companyId, kind, id);
        const d = await definitions(kind, row, actor);
        const shared = ports.hasPolicyReadSnapshot() ? await sharedFlowReferences(actor.companyId) : null;
        const route = shared ? shared.routes.get(`${kind}:${id}`) : await r.readReadFlowRoute(actor.companyId, kind, id);
        const saved = route ? JSON.parse(route.steps_json) as Record<string, SavedStep> : {};
        const defaultsRevision = route?.defaults_revision ?? shared?.revision ?? (await r.readReadFlowDefaultsRevision(actor.companyId))!.revision;
        const defaults = shared ? shared.defaults.filter(row => row.kind === kind && (row.branch_id === null || row.branch_id === d.scope.branch_id) && (row.department_id === null || row.department_id === d.scope.department_id) && row.revision <= defaultsRevision) : (await r.readReadFlowDefaults(actor.companyId, kind, d.scope.branch_id, d.scope.department_id, defaultsRevision)).results;
        const now = ports.policyNow();
        const people = await ports.policyRead(JSON.stringify(["flow-people", actor.companyId]), async () => (await r.readReadFlowPeople(actor.companyId, now, now)).results);
        const known = new Map(people.map(p => [p.user_id, { userId: p.user_id, name: p.legal_name, teamId: p.department_id, teamName: p.team_name ?? "Company" } satisfies FlowPerson]));
        // A company owner or invited specialist can own work before they have an HR
        // engagement. Keep role pools employment-backed, but retain explicitly named
        // owners while their account assignment and principal are active.
        const members = await ports.policyRead(JSON.stringify(["flow-members", actor.companyId]), async () => (await r.readReadFlowMembers(actor.companyId, now, now)).results);
        for (const member of members)
            if (!known.has(member.user_id))
                known.set(member.user_id, { userId: member.user_id, name: member.user_id === actor.userId && 'name' in actor && typeof actor.name === 'string' ? actor.name : member.name, teamId: null, teamName: "Company" });
        const historyRows = shared ? shared.histories.get(`${kind}:${id}`) ?? [] : (await r.readReadFlowHistoryRows(actor.companyId, kind, id)).results;
        const history = historyRows.map(e => ({ action: e.action, actorName: e.actor_name, at: e.created_at, reason: e.reason, step: e.step_key }));
        const identities = await ports.currentPersonPrincipals(actor.companyId, [...d.steps.flatMap(s => [...s.ids, ...(s.preferred ? [s.preferred] : [])]), ...Object.values(saved).flatMap(s => s.primaryUserId ? [s.primaryUserId] : []), ...defaults.map(d => d.primary_user_id)]);
        const steps: FlowStep[] = d.steps.map(s => {
            const candidates = [...new Set(s.ids.map(id => identities.get(id) ?? id))].flatMap(userId => known.has(userId) ? [known.get(userId)!] : []);
            const configured = defaults.find(v => v.step_key === s.key);
            const authoritative = (kind === "leave" || kind === "staff_task" || kind === "journal" || s.role === "personal_approver" || kind === "request" && s.key === "in_progress") ? s.preferred : undefined;
            const selected = authoritative ?? saved[s.key]?.primaryUserId ?? s.preferred ?? configured?.primary_user_id ?? (candidates.length === 1 ? candidates[0].userId : null);
            const primary = candidates.find(p => p.userId === (selected ? identities.get(selected) ?? selected : null)) ?? null;
            const status = !d.terminal && s.key === d.current ? "current" : history.some(e => e.step === s.key && !["assign", "configure"].includes(e.action)) || d.approvalSteps.some(a => (a.stage === s.key || `approval_${a.ordinal}` === s.key) && a.state === "approved") ? "recorded" : "upcoming";
            return { key: s.key, action: s.action, role: s.role, allowedRoles: s.allowedRoles, teamName: s.teamName ?? primary?.teamName ?? s.role.replaceAll("_", " "), primary, candidates, status, blocker: s.blocker ?? (primary ? null : selected ? "The primary staff member is no longer eligible. Reassign with a reason." : candidates.length ? "Select a primary staff member " + "from the eligible team." : "No eligible staff member. Configure active staff and role coverage.") };
        });
        const current = steps.find(s => s.status === "current") ?? null;
        const next = kind === "request" && current?.key === "in_progress" ? steps.find(s => s.key === "confirm") ?? null : kind === "request" && current?.key === "waiting_requester" ? steps.find(s => s.key === "in_progress") ?? null : current ? steps[steps.indexOf(current) + 1] ?? null : null;
        const blocker = d.terminal ? null : str(row.blocked_reason) || current?.blocker || (!current ? "This document requires routing repair before proceeding." : null);
        return { kind, recordId: id, recordVersion: Number(row.version), version: route?.version ?? 0, defaultsRevision, state: str(kind === "expense" ? row.status : row.state || row.status), steps, current, next, canAct: !blocker && current?.primary?.userId === actor.userId && flowRoleAllowed(actor, current), canManage: await ports.companyAuthority(actor), blocker, history };
    }
    async function routeStatements(actor: StaffActor, key: string, view: FlowView) {
        const args = [actor.companyId, view.kind, view.recordId];
        if (view.version)
            return [r.guard24(key, `flow-version-${view.kind}`, args, view.version)];
        const revision = view.defaultsRevision;
        const saved = Object.fromEntries(view.steps.map(s => [s.key, { primaryUserId: s.primary?.userId ?? null, teamName: s.teamName, defaultRevision: revision }]));
        return [r.guard25(key, `flow-defaults-version-${view.kind}`, actor.companyId, revision), r.stageRouteStatementsEffect(args, revision, JSON.stringify(saved), ports.clock.now())];
    }
    async function prepareFlowCommand(actor: StaffActor, key: string, kind: FlowKind, id: string, action: string, input: unknown, version: number): Promise<{
        before: Effect[];
        after: Effect[];
        view: FlowView;
    }> {
        const snap = await ports.expenseAuthoritySnapshot(actor.companyId);
        const view = await resolveFlow(actor, kind, id);
        if (kind === "order" && action === "invoice") {
            const step = view.steps.find(s => s.key === "invoicing") ?? null;
            view.current = step;
            view.blocker = step?.blocker ?? null;
            view.canAct = !view.blocker && step?.primary?.userId === actor.userId && flowRoleAllowed(actor, step);
        }
        if (kind === "payroll" && action === "calculate") {
            const step = view.steps.find(s => s.key === "calculate") ?? null;
            view.current = step;
            view.blocker = step?.blocker ?? null;
            view.canAct = !view.blocker && step?.primary?.userId === actor.userId && flowRoleAllowed(actor, step);
        }
        // Cancellation, rerouting and comments retain their stricter domain-specific policy.
        const administrative = ["cancel", "reverse", "reopen", "comment", "repair", "reroute"].includes(action.split(".").at(-1)!);
        const override = typeof input === "object" && input !== null && (input as {
            override?: boolean;
        }).override === true;
        const overrideAllowed = override && (["expense", "leave"].includes(kind) && (await ports.approvalView(kind as "expense" | "leave", id, actor)).canOverride
            || kind === "request" && await ports.canOverrideEmployeeRequest(actor, await ports.readRequestForOverride(actor.companyId, id)));
        if (!administrative && !view.canAct && !overrideAllowed)
            throw new BusinessRuleError(view.blocker ? 409 : 403, "FLOW_PRIMARY_REQUIRED", view.blocker ?? `This action belongs to ${view.current?.primary?.name ?? "the responsible staff member"}.`);
        const reason = typeof input === "object" && input !== null ? String((input as Record<string, unknown>).reason ?? (input as Record<string, unknown>).note ?? "") : "";
        const person = await r.readPrepareFlowCommandPerson(actor.companyId, actor.userId);
        const steps: Effect[] = [snap.guard(key, "flow-authority"), ...await routeStatements(actor, key, view), r.stagePrepareFlowCommandSteps(ports.identifiers.next(), key, actor.companyId, kind, id, view.current?.key ?? view.state, action, actor.userId, person?.legal_name ?? actor.userId, reason, version, ports.clock.now())];
        if (kind === "journal") {
            const parent = await r.readPrepareFlowCommandParent(id, actor.companyId);
            if (parent)
                steps.push(...await routeStatements(actor, key, await resolveFlow(actor, "order", parent.order_id)));
        }
        // The owner update is committed between the guards and this durable notification.
        const after = ports.flowAfterEffects(actor, key, kind, id);
        return { before: steps, after, view };
    }
    async function assignFlowPrimary(actor: StaffActor, kind: FlowKind, id: string, input: {
        stepKey: string;
        userId: string;
        expectedVersion: number;
        recordVersion: number;
        reason: string;
        saveDefault: boolean;
    }, key: string): Promise<Record<string, unknown>> {
        let featureGuards: Effect[] = [];
        if (ports.production) {
            if (kind === 'record')
                throw new BusinessRuleError(403, 'FEATURE_COMING_SOON', 'This workflow needs a separate pilot release');
            const owner = kind === 'order'
                ? (await flowRecord(actor.companyId, kind, id)).kind === 'purchase' ? 'procurement' : 'sales'
                : ({ expense: 'people_payroll', leave: 'people_payroll', payroll: 'people_payroll', staff_task: 'operations', request: 'operations', stock_count: 'inventory', transfer: 'inventory', invoice: 'finance', journal: 'finance' } as Record<string, string>)[kind];
            await ports.assertProductionModuleEnabled(actor.companyId, owner);
            featureGuards = ports.productionFeatureGuard(key, actor.companyId, owner);
        }
        const prior = await ports.commands.replay(actor, key, "flow.assign", id, { kind, ...input });
        if (prior)
            return prior;
        if (!await ports.companyAuthority(actor))
            throw new BusinessRuleError(403, "FLOW_ADMIN_REQUIRED", "A company administrator must assign workflow responsibilities.");
        const snap = await ports.expenseAuthoritySnapshot(actor.companyId);
        const view = await resolveFlow(actor, kind, id), step = view.steps.find(s => s.key === input.stepKey);
        if (view.version !== input.expectedVersion || view.recordVersion !== input.recordVersion)
            throw new BusinessRuleError(409, "VERSION_CONFLICT", "The workflow changed. Refresh before assigning.");
        if (!view.current || !step || view.steps.indexOf(step) < view.steps.indexOf(view.current))
            throw new BusinessRuleError(409, "FLOW_STEP_LOCKED", "Historical steps cannot be reassigned.");
        const person = step.candidates.find(p => p.userId === input.userId);
        if (!person)
            throw new BusinessRuleError(422, "FLOW_STAFF_INELIGIBLE", "Choose an active eligible staff member for this step.");
        const row = await flowRecord(actor.companyId, kind, id), scope = scopeFor(row);
        const existing = await r.readAssignFlowPrimaryExisting(actor.companyId, kind, id);
        const saved: Record<string, SavedStep> = existing ? JSON.parse(existing.steps_json) : Object.fromEntries(view.steps.map(s => [s.key, { primaryUserId: s.primary?.userId ?? null, teamName: s.teamName, defaultRevision: 0 }]));
        const linked: Effect[] = [];
        if (kind === "journal") {
            const parent = await r.readAssignFlowPrimaryParent(id, actor.companyId);
            if (parent) {
                const parentFlow = await resolveFlow(actor, "order", parent.order_id), parentStep = parentFlow.steps.find(s => s.key === "stock_gl");
                if (!parentStep?.candidates.some(p => p.userId === input.userId))
                    throw new BusinessRuleError(422, "FLOW_STAFF_INELIGIBLE", "The accountant must also be eligible for the source order.");
                linked.push(...await routeStatements(actor, key, parentFlow), r.stageAssignFlowPrimaryEffect(input.userId, actor.companyId, parent.order_id));
            }
        }
        const administrator = await r.readAssignFlowPrimaryAdministrator(actor.companyId, actor.userId);
        saved[step.key] = { ...saved[step.key], primaryUserId: person.userId, teamName: step.teamName };
        return ports.commands.commit(actor, key, "flow.assign", { kind, ...input }, id, view.recordVersion, { id, version: (view.version || 1) + 1 }, [
            ...featureGuards, ports.authorityGuard(key, actor), snap.guard(key, "flow-authority"), ...await routeStatements(actor, key, view), ...linked,
            r.guard34(key, "flow-document-version", actor.companyId, id, view.recordVersion, kind),
            r.stageAssignFlowPrimaryEffect2(JSON.stringify(saved), actor.companyId, kind, id),
            ...(input.saveDefault ? [r.stageAssignFlowPrimaryEffect3(actor.companyId, kind, step.key, scope.branch_id, scope.department_id, person.userId, actor.userId, input.reason, ports.clock.now())] : []),
            r.stageAssignFlowPrimaryEffect4(ports.identifiers.next(), key, actor.companyId, kind, id, step.key, actor.userId, administrator?.legal_name ?? actor.userId, `${input.reason} → ${person.name}`, view.recordVersion, ports.clock.now()),
            ports.notification(scope, person.userId, kind, id, view.recordVersion, `${step.action}: assigned to you`),
        ], { companyId: scope.company_id, branchId: scope.branch_id, departmentId: scope.department_id });
    }
    return { flowRecord, flowRoleAllowed, resolveFlow, prepareFlowCommand, assignFlowPrimary, flowCommandTarget };
}
