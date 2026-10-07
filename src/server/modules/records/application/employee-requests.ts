import type { namedRoute } from "../public/request-preview";
import { samePerson } from "@/lib/iam";
import { hasWorkflowPermission } from "@/lib/permissions";
import { z } from "zod";
import { categoryLabel, type RequestItem } from "@/lib/employee-services";
import type { WorkItem } from "@/lib/work-item";
import { quantityMilli } from "@/lib/commerce-math";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { ApprovalRoute } from "@/server/modules/operations/public/staff-policy";
import type { activeStaff } from "@/server/modules/operations/public/staff-policy";
import type { notification } from "@/server/modules/operations/public/staff-policy";
import type { prepareAttachmentBinding } from "@/server/modules/files/public/attachments/service";
import type { resourceRequestRoute } from "@/server/modules/people_payroll/public/expense-routing";
import type { expenseRoute } from "@/server/modules/people_payroll/public/expense-routing";
import type { spendingStepCandidates } from "@/server/modules/people_payroll/public/expense-routing";
import type { expenseAuthoritySnapshot } from "@/server/modules/people_payroll/public/expense-routing";
import type { departmentRoute } from "@/server/modules/organization/public/department-authority";
import type { departmentStepEligible } from "@/server/modules/organization/public/department-authority";
import type { canOverrideStaffStep } from "@/server/modules/workflow/public/approvals";
import type { staffOverrideGuards } from "@/server/modules/workflow/public/approvals";
import type { companyMember } from "@/server/modules/support/public/service-access";
import type { inServiceWorkspace } from "@/server/modules/support/public/service-access";
import type { serviceAuthoritySnapshot } from "@/server/modules/support/public/service-access";
import type { serviceMember } from "@/server/modules/support/public/service-access";
import type { serviceOwner } from "@/server/modules/support/public/service-access";
import type { validateServiceScope } from "@/server/modules/support/public/service-access";
import type { resolveServiceTeam } from "@/server/modules/support/public/service-teams";
import type { teamHandlers } from "@/server/modules/support/public/service-teams";
import { employeeRequestSchema, requestActionSchema, type EmployeeRequest, type SavedApproval, terminal, editable, requestOwn } from '../domain/employee-requests';
import { BusinessRuleError, type Clock, type IdentifierSource } from '../../shared';
import type { RecordsCommands } from '../domain/commands';
type Tail<Value extends unknown[]> = Value extends [
    unknown,
    ...infer Rest
] ? Rest : never;
type PortResult<Value, Effect> = Value extends Promise<infer Result> ? Promise<PortResult<Result, Effect>> : Value extends {
    bind(...args: never[]): unknown;
    run(): unknown;
} ? Effect : Value extends (...args: infer Args) => infer Result ? (...args: Args) => PortResult<Result, Effect> : Value extends readonly unknown[] ? {
    [Index in keyof Value]: PortResult<Value[Index], Effect>;
} : Value extends object ? {
    [Key in keyof Value]: PortResult<Value[Key], Effect>;
} : Value;
export interface EmployeeRequestsRepository<Effect> {
    guard1(key: string, value: "request-version", id: string, company_id: string, version: number, state: string, evidence_version: number): Effect;
    readReadRequestForOverrideRow(id: string, companyId: string): Promise<EmployeeRequest | null>;
    readReadEmployeeRequestRow(id: string, companyId: string): Promise<EmployeeRequest | null>;
    readRequestAttachmentsEffect(id: string): Promise<{
        results: ({
            id: string;
            original_name: string;
            media_type: string;
            bytes: number;
            checksum_sha256: string;
            purpose: string;
            revision: number;
        })[];
    }>;
    readEmployeeRequestDetailEvents(id: string, companyId: string): Promise<{
        results: ({
            id: string;
            action: string;
            actor_id: string;
            actor_name: string;
            note: string;
            evidence_json: string;
            request_version: number;
            created_at: number;
        })[];
    }>;
    readEmployeeRequestDetailTeam(team_id: string, companyId: string): Promise<{
        name: string;
    } | null>;
    readEmployeeRequestDetailChildren(id: string, companyId: string): Promise<{
        results: ({
            id: string;
            title: string;
            kind: string;
            state: string;
        })[];
    }>;
    stageEventEffect(value: string, company_id: string, id: string, version: number, action: string, userId: string, note: string, value2: string, now: number): Effect;
    stageSaveEmployeeRequestStatements(values: (string | null)[], version: number, now: number, recordId: string, companyId: string): Effect;
    stageSaveEmployeeRequestStatements2(recordId: string, companyId: string, value: string | null, values: (string | null)[], kind: "service" | "equipment" | "purchase", requester: string, userId: string, parentRequestId: string | null, version: number, now: number, now2: number): Effect;
    stageActOnEmployeeRequestEffect(engagement_id: string, id: string, company_id: string): Effect;
    readActOnEmployeeRequestAllocations(id: string): Promise<{
        results: ({
            ordinal: number;
            allocated_milli: number;
        })[];
    }>;
    readActOnEmployeeRequestOrders(company_id: string, branch_id: string | null, department_id: string | null, purchase_order_id: string, id: string): Promise<{
        results: ({
            id: string;
            state: string;
            version: number;
        })[];
    }>;
    guard15(key: string, value: string, id: string, company_id: string, version: number, state: string, branch_id: string | null, department_id: string | null): Effect;
    readActOnEmployeeRequestQuotationCount(id: string): Promise<Record<string, unknown> | null>;
    readActOnEmployeeRequestQuotation(id: string, purchaseOrderId: string): Promise<{
        id: string;
        order_id: string | null;
    } | null>;
    readActOnEmployeeRequestOrder(purchaseOrderId: string, company_id: string): Promise<{
        kind: string;
        company_id: string;
        branch_id: string;
        department_id: string | null;
        state: string;
        version: number;
    } | null>;
    guard20(key: string, value: "linked-order-version", purchaseOrderId: string, company_id: string, value2: number, value3: string, branch_id: string, department_id: string | null): Effect;
    readActOnEmployeeRequestLinked(id: string, purchaseOrderId: string): Promise<Record<string, unknown> | null>;
    stageActOnEmployeeRequestEffect2(id: string, purchaseOrderId: string, value: string | null, version: number, evidence_version: number, userId: string, now: number): Effect;
    readActOnEmployeeRequestEffect(company_id: string): Promise<{
        results: ({
            user_id: string;
        })[];
    }>;
    stageActOnEmployeeRequestEffect3(state: string, team_id: string | null, assignee_user_id: string | null, routing_json: string, approval_json: string, blocked_reason: string | null, resolution: string, purchase_order_id: string | null, handover_json: string, version: number, updated_at: number, id: string, company_id: string): Effect;
    readListEmployeeRequestsRows(companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null, value: 0 | 1, userId: string, userId2: string, userId3: string, userId4: string, userId5: string, userId6: string): Promise<{
        results: (EmployeeRequest)[];
    }>;
    readPrepareRequestOrderHandoffQuotation(requestId: string, rfqId: string): Promise<{
        id: string;
    } | null>;
    readPrepareRequestOrderHandoffActiveQuotationCount(requestId: string): Promise<Record<string, unknown> | null>;
    readPrepareRequestOrderHandoffExistingLink(requestId: string, orderId: string): Promise<{
        request_id: string;
    } | null>;
    stagePrepareRequestOrderHandoffEffect(orderId: string, now: number, id: string, company_id: string): Effect;
    stagePrepareRequestOrderHandoffEffect2(id: string, orderId: string, value: string | null, version: number, evidence_version: number, userId: string, now: number): Effect;
    readRequestQuotationSourceAllocated(id: string): Promise<{
        results: ({
            ordinal: number;
            allocated_milli: number;
        })[];
    }>;
}
export interface EmployeeRequestsPorts<Effect> {
    repository: EmployeeRequestsRepository<Effect>;
    clock: Clock;
    identifiers: IdentifierSource;
    commands: RecordsCommands<Effect>;
    prepare(holdingId: string): Promise<void>;
    production: boolean;
    batch(effects: Effect[]): Promise<unknown>;
    policyRead<Result>(key: string, read: () => Promise<Result>): Promise<Result>;
    policyNow(): number;
    namedRoute(...args: Tail<Parameters<typeof namedRoute>>): PortResult<ReturnType<typeof namedRoute>, Effect>;
    stageRouteCheck(key: string, step: string, check: NonNullable<ApprovalRoute['checks']>[number]): Effect;
    requirePermission(actor: WorkContext, permission: string): WorkContext;
    activeStaff(...args: Tail<Parameters<typeof activeStaff>>): PortResult<ReturnType<typeof activeStaff>, Effect>;
    notification(...args: Tail<Parameters<typeof notification>>): PortResult<ReturnType<typeof notification>, Effect>;
    prepareAttachmentBinding(...args: Tail<Parameters<typeof prepareAttachmentBinding>>): PortResult<ReturnType<typeof prepareAttachmentBinding>, Effect>;
    resourceRequestRoute(...args: Tail<Parameters<typeof resourceRequestRoute>>): PortResult<ReturnType<typeof resourceRequestRoute>, Effect>;
    expenseRoute(...args: Tail<Parameters<typeof expenseRoute>>): PortResult<ReturnType<typeof expenseRoute>, Effect>;
    spendingStepCandidates(...args: Tail<Parameters<typeof spendingStepCandidates>>): PortResult<ReturnType<typeof spendingStepCandidates>, Effect>;
    expenseAuthoritySnapshot(...args: Tail<Parameters<typeof expenseAuthoritySnapshot>>): PortResult<ReturnType<typeof expenseAuthoritySnapshot>, Effect>;
    departmentRoute(...args: Tail<Parameters<typeof departmentRoute>>): PortResult<ReturnType<typeof departmentRoute>, Effect>;
    departmentStepEligible(...args: Tail<Parameters<typeof departmentStepEligible>>): PortResult<ReturnType<typeof departmentStepEligible>, Effect>;
    canOverrideStaffStep(...args: Tail<Parameters<typeof canOverrideStaffStep>>): PortResult<ReturnType<typeof canOverrideStaffStep>, Effect>;
    staffOverrideGuards(...args: Tail<Parameters<typeof staffOverrideGuards>>): PortResult<ReturnType<typeof staffOverrideGuards>, Effect>;
    companyMember(...args: Tail<Parameters<typeof companyMember>>): PortResult<ReturnType<typeof companyMember>, Effect>;
    inServiceWorkspace(...args: Parameters<typeof inServiceWorkspace>): PortResult<ReturnType<typeof inServiceWorkspace>, Effect>;
    serviceAuthoritySnapshot(...args: Tail<Parameters<typeof serviceAuthoritySnapshot>>): PortResult<ReturnType<typeof serviceAuthoritySnapshot>, Effect>;
    serviceMember(...args: Tail<Parameters<typeof serviceMember>>): PortResult<ReturnType<typeof serviceMember>, Effect>;
    serviceOwner(...args: Tail<Parameters<typeof serviceOwner>>): PortResult<ReturnType<typeof serviceOwner>, Effect>;
    validateServiceScope(...args: Tail<Parameters<typeof validateServiceScope>>): PortResult<ReturnType<typeof validateServiceScope>, Effect>;
    resolveServiceTeam(...args: Tail<Parameters<typeof resolveServiceTeam>>): PortResult<ReturnType<typeof resolveServiceTeam>, Effect>;
    teamHandlers(...args: Tail<Parameters<typeof teamHandlers>>): PortResult<ReturnType<typeof teamHandlers>, Effect>;
}
export function createEmployeeRequestsApplication<Effect>(ports: EmployeeRequestsPorts<Effect>) {
    const r = ports.repository;
    function requestVersionGuard(key: string, row: EmployeeRequest) {
        return r.guard1(key, "request-version", row.id, row.company_id, row.version, row.state, row.evidence_version);
    }
    async function canOverrideEmployeeRequest(actor: Pick<WorkContext, "companyId" | "userId" | "role" | "assignmentId">, row: EmployeeRequest) {
        const saved = JSON.parse(row.approval_json) as SavedApproval, step = saved.route?.steps[saved.currentStep ?? 0];
        if (row.state !== "awaiting_approval" || step?.stage !== "department_approval")
            return false;
        return ports.canOverrideStaffStep({ companyId: row.company_id, branchId: row.branch_id, departmentId: row.department_id,
            requesterUserId: row.requester_user_id, preparerUserId: row.created_by, assignedUserId: step.userId }, actor);
    }
    async function readRequestForOverride(companyId: string, id: string) {
        const row = await r.readReadRequestForOverrideRow(id, companyId);
        if (!row)
            throw new BusinessRuleError(404, "REQUEST_NOT_FOUND", "Request not found.");
        return row;
    }
    async function requestAccess(actor: WorkContext, row: EmployeeRequest, override = false) {
        const saved = JSON.parse(row.approval_json) as SavedApproval;
        const current = saved.route?.steps[saved.currentStep ?? 0];
        const designated = !!current?.snapshot?.departmentPolicyVersion && samePerson(actor, current.userId)
            && await ports.departmentStepEligible(row.company_id, actor.userId, current.snapshot);
        const superior = override && await canOverrideEmployeeRequest(actor, row);
        if (!designated && !superior && (!ports.inServiceWorkspace(actor, row) ||
            !(await ports.serviceMember(row, actor.userId))))
            throw new BusinessRuleError(404, "REQUEST_NOT_FOUND", "This request is outside your current workspace.");
        const owner = await ports.serviceOwner(actor), handlers = await ports.teamHandlers(row, row.team_id), member = handlers.find((h) => h.user_id === actor.userId);
        let route: ApprovalRoute | null = null, pool: string[] = [];
        if (row.state === "awaiting_approval") {
            if (saved.route?.steps.some(s => s.snapshot?.spendingPolicy || s.snapshot?.departmentPolicyVersion !== undefined)) {
                route = saved.route;
                const step = route.steps[saved.currentStep ?? 0];
                pool = step ? await ports.spendingStepCandidates(row, row.requester_user_id, [row.created_by], { ...step, candidateUserIds: saved.pool }, (saved.decisions ?? []).map(d => d.userId)) : [];
            }
            else {
                route = await ports.resourceRequestRoute(row.company_id, row.requester_user_id, row.created_by);
                pool = (route.steps[0]?.candidateUserIds ?? []).filter(id => saved.pool?.includes(id));
            }
        }
        if (superior)
            pool = [actor.userId];
        const own = requestOwn(actor, row), approver = !!superior || pool.some(id => samePerson(actor, id)) && (route?.steps[saved.currentStep ?? 0]?.snapshot?.departmentPolicyVersion !== undefined || hasWorkflowPermission(actor, route?.steps[saved.currentStep ?? 0]?.requiredRole ?? "people_manager"));
        if (!own &&
            !owner &&
            !member &&
            !approver &&
            !saved.decisions?.some(d => samePerson(actor, d.userId)) && !samePerson(actor, saved.decidedBy))
            throw new BusinessRuleError(404, "REQUEST_NOT_FOUND", "This request is not available to you.");
        return { owner, own, member, handlers, pool, route, approver, saved };
    }
    async function readEmployeeRequest(actor: WorkContext, id: string) {
        const row = await r.readReadEmployeeRequestRow(id, actor.companyId);
        if (!row)
            throw new BusinessRuleError(404, "REQUEST_NOT_FOUND", "Request not found.");
        return row;
    }
    async function getEmployeeRequest(actor: WorkContext, id: string, override = false) {
        const row = await readEmployeeRequest(actor, id);
        await requestAccess(actor, row, override);
        return row;
    }
    async function requestAttachments(id: string) {
        return (await r.readRequestAttachmentsEffect(id)).results;
    }
    async function employeeRequestDetail(actor: WorkContext, id: string, includeEvidence = true, override = false) {
        const row = await readEmployeeRequest(actor, id), access = await requestAccess(actor, row, override);
        const blocked = row.state === "awaiting_approval" && !access.pool.length
            ? (access.route?.blockedReason ??
                "The independent approver is unavailable. Resolve the approval issue.")
            : row.team_id &&
                !terminal.includes(row.state) &&
                !editable.includes(row.state) &&
                row.state !== "awaiting_approval" &&
                (!access.handlers.some((h) => h.responsibility === "lead") ||
                    (row.assignee_user_id &&
                        !access.handlers.some((h) => h.user_id === row.assignee_user_id)))
                ? "The assigned team or handler is unavailable. Reassign this request in Administration."
                : row.blocked_reason;
        const actions: string[] = [];
        if (access.own && editable.includes(row.state))
            actions.push("edit", "submit", "cancel");
        if (access.own && row.state === "blocked")
            actions.push("cancel");
        if (access.approver)
            actions.push("approve", "return", "reject");
        if (access.member?.responsibility === "lead" &&
            ["queued", "in_progress", "waiting_requester"].includes(row.state))
            actions.push("assign");
        if (!terminal.includes(row.state) && !editable.includes(row.state))
            actions.push("comment");
        const handler = !!access.member && samePerson(actor, row.assignee_user_id);
        if (handler && row.state === "queued")
            actions.push("start");
        if (handler && row.state === "in_progress")
            actions.push("ask", "resolve", ...(row.kind === "purchase" ? ["link-order"] : []));
        if (access.own && row.state === "waiting_requester")
            actions.push("reply");
        if (actor.userId === row.requester_user_id &&
            ["resolved", "fulfilled"].includes(row.state))
            actions.push("confirm", "reopen");
        if (access.owner &&
            !terminal.includes(row.state) &&
            !editable.includes(row.state))
            actions.push("repair");
        if (access.member && row.kind === "service" && !terminal.includes(row.state))
            actions.push("resource-request");
        const events = includeEvidence
            ? (await r.readEmployeeRequestDetailEvents(id, actor.companyId)).results
            : [];
        const team = row.team_id
            ? await r.readEmployeeRequestDetailTeam(row.team_id, actor.companyId)
            : null;
        const children = includeEvidence
            ? (await r.readEmployeeRequestDetailChildren(id, actor.companyId)).results
            : [];
        const visibleChildren = [];
        for (const child of children) {
            try {
                await getEmployeeRequest(actor, child.id);
                visibleChildren.push(child);
            }
            catch (e) {
                if (!(e instanceof BusinessRuleError) || e.status !== 404)
                    throw e;
            }
        }
        const approvalStage = access.saved.route?.steps[access.saved.currentStep ?? 0]?.stage;
        const approvalLabel = approvalStage === "accounts_review" ? "Accounting preparation" : approvalStage === "accounts_approval" ? "Accounting approval (Level 1–4)" : "Supervisor approval";
        const responsible = row.state === "awaiting_approval"
            ? approvalLabel
            : row.state === "waiting_requester" ||
                ["resolved", "fulfilled"].includes(row.state)
                ? "Requester"
                : editable.includes(row.state)
                    ? "Requester"
                    : (team?.name ?? "Service setup");
        const next = blocked
            ? "Resolve routing issue"
            : row.state === "awaiting_approval"
                ? approvalStage === "accounts_review" ? "Prepare planned spending details" : approvalStage === "accounts_approval" ? "Approve planned spending before ordering" : "Review purpose and requested resources"
                : row.state === "queued"
                    ? row.assignee_user_id
                        ? "Start work"
                        : "Assign a handler"
                    : row.state === "in_progress"
                        ? row.kind === "purchase"
                            ? "Prepare the purchase order and record fulfillment"
                            : row.kind === "equipment"
                                ? "Record the equipment handover"
                                : "Resolve the issue"
                        : row.state === "waiting_requester"
                            ? "Reply to the team"
                            : row.state === "resolved" || row.state === "fulfilled"
                                ? "Confirm completion or reopen"
                                : editable.includes(row.state)
                                    ? "Complete and submit your request"
                                    : terminal.includes(row.state)
                                        ? "No further action"
                                        : "Resolve routing issue";
        return {
            request: row,
            items: JSON.parse(row.items_json) as RequestItem[],
            actions,
            blockedReason: blocked,
            responsibleQueue: responsible,
            assigneeName: row.assignee_user_id
                ? (access.handlers.find((h) => h.user_id === row.assignee_user_id)
                    ?.name ?? "Assigned handler unavailable")
                : null,
            nextAction: next,
            handlers: access.member || access.owner ? access.handlers : [],
            approvers: access.owner ? access.pool : [],
            events,
            attachments: includeEvidence ? await requestAttachments(id) : [],
            children: visibleChildren,
            relationship: {
                own: access.own,
                assigned: handler,
                approval: access.approver,
                team: !!access.member,
            },
        };
    }
    function event(row: EmployeeRequest, actor: WorkContext, action: string, note: string, evidence: unknown = {}) {
        return r.stageEventEffect(ports.identifiers.next(), row.company_id, row.id, row.version, action, actor.userId, note, JSON.stringify(evidence), ports.clock.now());
    }
    async function requestNotice(row: EmployeeRequest, actor: WorkContext, message: string, extra: string[] = []) {
        const recipients = new Set([row.requester_user_id, row.created_by, ...extra]);
        recipients.delete(actor.userId);
        const statements = [];
        for (const user of recipients)
            if (await ports.serviceMember(row, user))
                statements.push(ports.notification(row, user, "request", row.id, row.version, message));
        return statements;
    }
    async function saveEmployeeRequest(actor: WorkContext, id: string | null, input: z.infer<typeof employeeRequestSchema>, key: string) {
        const snapshot = await ports.serviceAuthoritySnapshot(actor.companyId), recordId = id ?? `request-${key}`, operation = id ? "employee.request.update" : "employee.request.create";
        if (!(await ports.companyMember(actor.companyId, actor.userId)))
            throw new BusinessRuleError(403, "MEMBERSHIP_REQUIRED", "Current company membership is required.");
        const prior = await ports.commands.replay<{
            id: string;
            version: number;
            state: string;
        }>(actor, key, operation, recordId, input);
        if (prior)
            return prior;
        const current = id ? await getEmployeeRequest(actor, id) : null;
        if (current &&
            (!requestOwn(actor, current) || !editable.includes(current.state)))
            throw new BusinessRuleError(403, "REQUEST_LOCKED", "Only the requester or preparer can edit an unsubmitted request.");
        if (current && current.version !== input.expectedVersion)
            throw new BusinessRuleError(409, "VERSION_CONFLICT", "This request changed. Review the current version before saving.");
        if (current &&
            (input.kind !== current.kind ||
                input.parentRequestId !== current.parent_request_id))
            throw new BusinessRuleError(422, "REQUEST_IDENTITY_LOCKED", "Request type and original service link cannot change.");
        let requester = current?.requester_user_id ?? actor.userId;
        if (!current && input.parentRequestId) {
            const parent = await getEmployeeRequest(actor, input.parentRequestId), a = await requestAccess(actor, parent);
            if (parent.kind !== "service" ||
                input.kind === "service" ||
                (!a.member && !a.own) ||
                terminal.includes(parent.state))
                throw new BusinessRuleError(403, "RESOURCE_LINK_FORBIDDEN", "Create a resource request from an active service job you handle or requested.");
            requester = parent.requester_user_id;
        }
        const engagement = input.kind === "purchase" ? await ports.activeStaff(actor.companyId, requester) : null;
        if (current?.engagement_id && engagement && current.engagement_id !== engagement.engagement_id)
            throw new BusinessRuleError(409, "ENGAGEMENT_CHANGED", "Start a new request for the current engagement.");
        const scope = {
            company_id: actor.companyId,
            branch_id: input.branchId ?? actor.branchId,
            department_id: input.departmentId ?? actor.departmentId,
        };
        await ports.validateServiceScope(actor, scope);
        if (!(await ports.serviceMember(scope, requester)))
            throw new BusinessRuleError(422, "REQUESTER_SCOPE_REQUIRED", "The requester needs access covering this location.");
        const category = input.kind === "equipment"
            ? "operations"
            : input.kind === "purchase"
                ? "purchasing"
                : input.category;
        const version = current ? current.version + 1 : 1, now = ports.clock.now();
        const values = [
            scope.branch_id,
            scope.department_id,
            category,
            input.title,
            input.description,
            input.location,
            input.impact,
            input.neededDate || null,
            JSON.stringify(input.items),
            input.currency,
        ];
        const row = { id: recordId, ...scope, version } as EmployeeRequest;
        const statements = [
            ...snapshot(key),
            ...(current
                ? [
                    requestVersionGuard(key, current),
                    r.stageSaveEmployeeRequestStatements(values, version, now, recordId, actor.companyId),
                ]
                : [
                    r.stageSaveEmployeeRequestStatements2(recordId, actor.companyId, engagement?.engagement_id ?? null, values, input.kind, requester, actor.userId, input.parentRequestId, version, now, now),
                ]),
            event(row, actor, current ? "edit" : "create", "", {
                fields: input,
                requester,
            }),
        ];
        return ports.commands.commit(actor, key, operation, input, recordId, version, { id: recordId, version, state: "draft" }, statements, {
            companyId: scope.company_id,
            branchId: scope.branch_id,
            departmentId: scope.department_id,
        });
    }
    async function submittedRoute(row: EmployeeRequest) {
        if (row.title.length < 2 ||
            row.description.length < 2 ||
            !row.location.trim())
            throw new BusinessRuleError(422, "REQUEST_DETAILS_REQUIRED", "Add a title, description or purpose, and location before submitting.");
        if (row.kind === "service")
            return null;
        const items = JSON.parse(row.items_json) as RequestItem[];
        if (!items.length ||
            items.some((i) => !i.description.trim() ||
                !Number.isFinite(Number(i.quantity)) ||
                Number(i.quantity) <= 0) ||
            (row.kind === "equipment" && !row.needed_date))
            throw new BusinessRuleError(422, "RESOURCE_DETAILS_REQUIRED", row.kind === "equipment"
                ? "Add the requested items, positive quantities, and needed date."
                : "Add the requested items and positive quantities.");
        if (items.some((i) => i.estimatedCost) && !row.currency)
            throw new BusinessRuleError(422, "CURRENCY_REQUIRED", "Choose a currency for estimated costs.");
        const staff = await ports.activeStaff(row.company_id, row.requester_user_id);
        if (staff.branch_id !== row.branch_id ||
            staff.department_id !== row.department_id)
            throw new BusinessRuleError(409, "REQUESTER_LOCATION_REQUIRED", "Use the requester’s active branch and department. Review their People profile.");
        return row.kind === "purchase" ? ports.expenseRoute(row.company_id, row.requester_user_id, [row.created_by], false, "purchase") : ports.departmentRoute(row.company_id, row.requester_user_id, [row.created_by]);
    }
    async function actOnEmployeeRequest(actor: WorkContext, id: string, input: z.infer<typeof requestActionSchema>, key: string) {
        const operation = `employee.request.${input.action}`;
        const prior = await ports.commands.replay<{
            id: string;
            version: number;
            state: string;
        }>(actor, key, operation, id, input);
        if (prior)
            return prior;
        const snapshot = await ports.serviceAuthoritySnapshot(actor.companyId);
        const spendingSnapshot = await ports.expenseAuthoritySnapshot(actor.companyId);
        if (input.override && !["approve", "return", "reject"].includes(input.action))
            throw new BusinessRuleError(422, "OVERRIDE_ACTION_INVALID", "Only a pending department decision can be overridden.");
        const current = await getEmployeeRequest(actor, id, input.override), detail = await employeeRequestDetail(actor, id, false, input.override);
        if (current.version !== input.expectedVersion)
            throw new BusinessRuleError(409, "VERSION_CONFLICT", "The request changed. Refresh and review before continuing.");
        if (!detail.actions.includes(input.action))
            throw new BusinessRuleError(403, "REQUEST_ACTION_FORBIDDEN", "This action is not available to you at the current step.");
        if (!["approve", "return", "reject"].includes(input.action) || !detail.actions.includes(input.action))
            await ports.validateServiceScope(actor, current);
        const row = {
            ...current,
            version: current.version + 1,
            updated_at: ports.clock.now(),
        }, evidence: Record<string, unknown> = {}, extra: string[] = [];
        const statements: Effect[] = [
            ...snapshot(key),
            requestVersionGuard(key, current),
            spendingSnapshot.guard(key, "purchase-approval-authority"),
        ];
        const attachmentSetIds = input.attachmentSetIds ?? [];
        if (attachmentSetIds.length) {
            if (!["submit", "resolve"].includes(input.action))
                throw new BusinessRuleError(422, "REQUEST_ATTACHMENT_ACTION", "Bind request evidence when submitting or recording fulfillment.");
            const purposes = input.action === "submit" ? ["request_evidence", "it_request_evidence"] : ["fulfillment"];
            const binding = await ports.prepareAttachmentBinding(actor, { attachmentSetIds, bindingContract: { targetType: "employee_request", bindingType: input.action === "submit" ? "request_submission" : "request_fulfillment", contexts: purposes.map(purposeKey => ({ purposeKey, componentKind: "action" as const, componentRef: `request:${input.action}` })) }, targetId: id, expectedTargetVersion: current.version, commandId: key, bindingId: `${id}:${input.action}:${row.version}` });
            statements.push(...binding.guards, ...binding.steps);
            evidence.attachmentSetIds = attachmentSetIds;
        }
        if (input.override) {
            const saved = JSON.parse(current.approval_json) as SavedApproval, step = saved.route?.steps[saved.currentStep ?? 0];
            if (!step || step.stage !== "department_approval")
                throw new BusinessRuleError(409, "OVERRIDE_STEP_CHANGED", "The department step changed. Reload before deciding.");
            statements.push(...await ports.staffOverrideGuards(key, { companyId: current.company_id, branchId: current.branch_id, departmentId: current.department_id,
                requesterUserId: current.requester_user_id, preparerUserId: current.created_by, assignedUserId: step.userId }, actor));
        }
        const requiredNote = [
            "approve",
            "return",
            "reject",
            "ask",
            "reply",
            "resolve",
            "reopen",
            "repair",
            "link-order",
            "comment",
        ];
        if (requiredNote.includes(input.action) && input.note.length < 2)
            throw new BusinessRuleError(422, "REASON_REQUIRED", "Record a short reason or evidence for this action.");
        async function routeTeam(explicit: string | null = null) {
            const resolved = await ports.resolveServiceTeam(row, row.category);
            if (explicit && resolved?.team.id !== explicit)
                throw new BusinessRuleError(422, "TEAM_COVERAGE_REQUIRED", "Choose the currently eligible team covering this category and location.");
            row.assignee_user_id = null;
            if (!resolved) {
                row.state = "blocked";
                row.team_id = null;
                row.blocked_reason = `Configure ${categoryLabel(row.category)} coverage for this location in Administration.`;
                row.routing_json = "{}";
                return;
            }
            row.team_id = resolved.team.id;
            row.state = "queued";
            row.blocked_reason = null;
            row.routing_json = JSON.stringify({
                teamId: resolved.team.id,
                teamVersion: resolved.team.version,
                scopeKind: resolved.team.scope_kind,
                scopeNodeId: resolved.team.scope_node_id,
                category: row.category,
                at: ports.clock.now(),
            });
            extra.push(...resolved.handlers
                .filter((h) => h.responsibility === "lead")
                .map((h) => h.user_id));
        }
        if (input.action === "submit") {
            if (row.kind === 'purchase') {
                const staff = await ports.activeStaff(row.company_id, row.requester_user_id);
                if (row.engagement_id && row.engagement_id !== staff.engagement_id)
                    throw new BusinessRuleError(409, 'ENGAGEMENT_CHANGED', 'Start a new purchase request for the current engagement.');
                statements.push(r.stageActOnEmployeeRequestEffect(staff.engagement_id, row.id, row.company_id));
            }
            const route = await submittedRoute(row);
            if (route) {
                row.state = route.blockedReason ? "blocked" : "awaiting_approval";
                row.blocked_reason = route.blockedReason;
                row.approval_json = JSON.stringify({
                    pool: route.steps[0]?.candidateUserIds ?? (route.steps[0] ? [route.steps[0].userId] : []),
                    route: { ...route, checks: undefined },
                    currentStep: 0, decisions: [],
                    version: row.version,
                    evidenceVersion: row.evidence_version,
                });
                extra.push(...(route.steps[0]?.candidateUserIds ?? (route.steps[0] ? [route.steps[0].userId] : [])));
                for (const [i, c] of (route.checks ?? []).entries())
                    statements.push(ports.stageRouteCheck(key, `request-route-${i}`, c));
            }
            else
                await routeTeam();
        }
        else if (input.action === "approve") {
            const access = await requestAccess(actor, current, input.override);
            if (!access.pool.some(id => samePerson(actor, id)))
                throw new BusinessRuleError(403, "APPROVER_UNAVAILABLE", "Current independent department authority is required.");
            const strict = access.saved.route?.steps.some(s => s.snapshot?.spendingPolicy || s.snapshot?.departmentPolicyVersion !== undefined);
            const currentStep = access.saved.currentStep ?? 0;
            const original = access.saved.route?.steps[currentStep]?.userId;
            const decisions = [...(access.saved.decisions ?? []), { stage: access.saved.route?.steps[currentStep]?.stage ?? "department_approval", userId: actor.userId, note: input.note, at: ports.clock.now(), version: current.version,
                    ...(input.override ? { override: true, originalApproverUserId: original, actualApproverUserId: actor.userId } : {}) }];
            const next = strict ? access.saved.route?.steps[currentStep + 1] : null;
            row.approval_json = JSON.stringify({ ...access.saved, decisions, currentStep: currentStep + 1, pool: next?.candidateUserIds ?? (next ? [next.userId] : []), decision: next ? "pending" : "approved", decidedBy: actor.userId, reviewedVersion: current.version, evidenceVersion: current.evidence_version, decisionAt: ports.clock.now() });
            evidence.approval = JSON.parse(row.approval_json);
            evidence.attachments = await requestAttachments(id);
            if (next) {
                row.state = "awaiting_approval";
                extra.push(...(next.candidateUserIds ?? [next.userId]));
            }
            else
                await routeTeam();
        }
        else if (input.action === "return" || input.action === "reject") {
            row.state = input.action === "return" ? "returned" : "rejected";
            row.team_id = null;
            row.assignee_user_id = null;
            row.blocked_reason = null;
            const saved = JSON.parse(row.approval_json) as SavedApproval, original = saved.route?.steps[saved.currentStep ?? 0]?.userId;
            row.approval_json = JSON.stringify({
                ...saved,
                decision: input.action,
                decidedBy: actor.userId,
                reviewedVersion: current.version,
                decisions: [...(saved.decisions ?? []), { stage: saved.route?.steps[saved.currentStep ?? 0]?.stage ?? "department_approval", userId: actor.userId, note: input.note, at: ports.clock.now(), version: current.version,
                        ...(input.override ? { override: true, originalApproverUserId: original, actualApproverUserId: actor.userId } : {}) }],
            });
            evidence.approval = JSON.parse(row.approval_json);
        }
        else if (input.action === "assign") {
            if (!input.assigneeUserId ||
                !detail.handlers.some((h) => h.user_id === input.assigneeUserId))
                throw new BusinessRuleError(422, "HANDLER_INVALID", "Choose a current team member with access covering this request.");
            row.assignee_user_id = input.assigneeUserId;
            extra.push(input.assigneeUserId);
            evidence.previousAssignee = current.assignee_user_id;
            evidence.assigneeUserId = input.assigneeUserId;
            row.blocked_reason = null;
        }
        else if (input.action === "start") {
            row.state = "in_progress";
        }
        else if (input.action === "ask") {
            row.state = "waiting_requester";
        }
        else if (input.action === "reply") {
            row.state = "in_progress";
            if (row.assignee_user_id)
                extra.push(row.assignee_user_id);
        }
        else if (input.action === "resolve") {
            if (row.kind === "purchase") {
                if (!row.purchase_order_id)
                    throw new BusinessRuleError(422, "PURCHASE_HANDOFF_REQUIRED", "Link the approved purchase request to its purchase order before confirming fulfillment.");
                const allocations = (await r.readActOnEmployeeRequestAllocations(row.id)).results;
                if (allocations.length) {
                    const byOrdinal = new Map(allocations.map(x => [Number(x.ordinal), Number(x.allocated_milli)])), items = JSON.parse(row.items_json) as RequestItem[];
                    if (items.some((item, index) => quantityMilli(item.quantity) !== (byOrdinal.get(index) ?? 0)))
                        throw new BusinessRuleError(409, "PURCHASE_ALLOCATION_INCOMPLETE", "Allocate every approved request quantity before confirming fulfillment.");
                }
                const orders = (await r.readActOnEmployeeRequestOrders(row.company_id, row.branch_id, row.department_id, row.purchase_order_id, row.id)).results;
                if (!orders.length || orders.some(order => !["fulfilled", "invoiced"].includes(order.state)))
                    throw new BusinessRuleError(409, "PURCHASE_NOT_RECEIVED", "Record completed receipt through every linked purchase order before confirming fulfillment.");
                for (const [index, order] of orders.entries())
                    statements.push(r.guard15(key, `received-order-version-${index}`, order.id, row.company_id, order.version, order.state, row.branch_id, row.department_id));
            }
            row.state = row.kind === "service" ? "resolved" : "fulfilled";
            row.resolution = input.note;
            if (row.kind === "equipment") {
                row.handover_json = JSON.stringify({
                    handedBy: actor.userId,
                    receivedBy: row.requester_user_id,
                    assetReference: input.assetReference,
                    items: JSON.parse(row.items_json),
                    note: input.note,
                    at: ports.clock.now(),
                });
                evidence.handover = JSON.parse(row.handover_json);
            }
        }
        else if (input.action === "confirm") {
            row.state = "closed";
            evidence.confirmedBy = actor.userId;
        }
        else if (input.action === "reopen") {
            row.state = "in_progress";
            row.resolution = "";
            if (row.assignee_user_id)
                extra.push(row.assignee_user_id);
        }
        else if (input.action === "cancel") {
            row.state = "cancelled";
            row.blocked_reason = null;
        }
        else if (input.action === "repair") {
            evidence.previousRouting = JSON.parse(current.routing_json);
            evidence.previousApproval = JSON.parse(current.approval_json);
            const approval = JSON.parse(row.approval_json) as SavedApproval;
            if (row.kind !== "service" && approval.decision !== "approved") {
                const route = await submittedRoute(row);
                const pool = route?.steps[0]?.candidateUserIds ?? [];
                if (!pool.length)
                    throw new BusinessRuleError(409, "ROUTING_SETUP_REQUIRED", route?.blockedReason ??
                        "Configure an eligible independent department approver.");
                if (input.approverUserId && !pool.includes(input.approverUserId))
                    throw new BusinessRuleError(422, "REPLACEMENT_INELIGIBLE", "The replacement must have current independent department authority.");
                row.approval_json = JSON.stringify({
                    pool: input.approverUserId ? [input.approverUserId] : pool,
                    route: { ...route, checks: undefined },
                    currentStep: 0, decisions: [],
                    repairedBy: actor.userId,
                    version: row.version,
                    evidenceVersion: row.evidence_version,
                });
                for (const [i, c] of (route?.checks ?? []).entries())
                    statements.push(ports.stageRouteCheck(key, `request-repair-${i}`, c));
                row.state = "awaiting_approval";
                row.blocked_reason = null;
                extra.push(...(input.approverUserId ? [input.approverUserId] : pool));
            }
            else
                await routeTeam(input.teamId);
        }
        else if (input.action === "link-order") {
            if (actor.beta)
                throw new BusinessRuleError(403, "BETA_CAPABILITY_UNAVAILABLE", "Purchasing execution is unavailable in this beta workspace.");
            actor = ports.requirePermission(actor, "commerce.purchase");
            const quotationCount = Number(await r.readActOnEmployeeRequestQuotationCount(row.id));
            const quotation = input.purchaseOrderId ? await r.readActOnEmployeeRequestQuotation(row.id, input.purchaseOrderId) : null;
            if (quotationCount && !quotation)
                throw new BusinessRuleError(409, "REQUEST_RFQ_REQUIRED", "The linked purchase order must come from the active approved supplier award.");
            if (!input.purchaseOrderId)
                throw new BusinessRuleError(422, "ORDER_REQUIRED", "Choose a purchase order.");
            const order = await r.readActOnEmployeeRequestOrder(input.purchaseOrderId, row.company_id);
            if (!order)
                throw new BusinessRuleError(404, "ORDER_NOT_FOUND", "Purchase order not found.");
            if (order.kind !== "purchase" ||
                order.company_id !== row.company_id ||
                order.branch_id !== row.branch_id ||
                order.department_id !== row.department_id ||
                ["cancelled"].includes(String(order.state)))
                throw new BusinessRuleError(422, "ORDER_SCOPE_INVALID", "Choose a purchase order for the same company, branch, and department.");
            statements.push(r.guard20(key, "linked-order-version", input.purchaseOrderId, row.company_id, Number(order.version), String(order.state), row.branch_id, row.department_id));
            const linked = await r.readActOnEmployeeRequestLinked(row.id, input.purchaseOrderId);
            if (!linked)
                statements.push(r.stageActOnEmployeeRequestEffect2(row.id, input.purchaseOrderId, quotation?.id ?? null, row.version, row.evidence_version, actor.userId, ports.clock.now()));
            row.purchase_order_id = row.purchase_order_id ?? input.purchaseOrderId;
            evidence.purchaseOrderId = input.purchaseOrderId;
        }
        if (input.action === "comment" && row.assignee_user_id)
            extra.push(row.assignee_user_id);
        if (row.state === "blocked")
            extra.push(...(await r.readActOnEmployeeRequestEffect(row.company_id)).results.map((o) => o.user_id));
        statements.push(r.stageActOnEmployeeRequestEffect3(row.state, row.team_id, row.assignee_user_id, row.routing_json, row.approval_json, row.blocked_reason, row.resolution, row.purchase_order_id, row.handover_json, row.version, row.updated_at, id, row.company_id), event(row, actor, input.action, input.note, {
            ...evidence,
            routing: JSON.parse(row.routing_json),
            from: current.state,
            to: row.state,
        }), ...(await requestNotice(row, actor, `${row.title || "Request"} · ${input.action === "ask" ? "Your reply is needed" : input.action === "assign" ? "Assigned to you" : row.state.replaceAll("_", " ")}`, extra)));
        return ports.commands.commit(actor, key, operation, input, id, row.version, { id, version: row.version, state: row.state }, statements, {
            companyId: row.company_id,
            branchId: row.branch_id,
            departmentId: row.department_id,
        });
    }
    async function listEmployeeRequests(actor: WorkContext) {
        const owner = await ports.serviceOwner(actor);
        const rows = (await r.readListEmployeeRequestsRows(actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId, owner ? 1 : 0, actor.userId, actor.userId, actor.userId, actor.userId, actor.userId, actor.userId)).results;
        const visible = [];
        for (const r of rows) {
            try {
                await requestAccess(actor, r);
                visible.push(r);
            }
            catch (e) {
                if (!(e instanceof BusinessRuleError) || e.status !== 404)
                    throw e;
            }
        }
        return visible;
    }
    async function employeeRequestWork(actor: WorkContext): Promise<WorkItem[]> {
        const items: WorkItem[] = [];
        for (const row of await listEmployeeRequests(actor)) {
            const detail = await employeeRequestDetail(actor, row.id, false), r = detail.relationship;
            // Owners see routing exceptions, not every colleague's routine work.
            if (!r.own &&
                !r.assigned &&
                !r.approval &&
                !r.team &&
                !detail.blockedReason)
                continue;
            const actionable = detail.actions.some((a) => !["repair", "resource-request", "link-order", "comment"].includes(a)) ||
                (!!detail.blockedReason && detail.actions.includes("repair"));
            items.push({
                id: row.id,
                type: row.kind === "service"
                    ? "service_request"
                    : row.kind === "equipment"
                        ? "equipment_request"
                        : "purchase_request",
                title: row.title || "Untitled request",
                detail: detail.nextAction,
                href: `/requests/${row.id}`,
                status: row.state,
                updatedAt: row.updated_at,
                canAct: actionable,
                actionKind: detail.actions.includes("approve") && detail.responsibleQueue !== "Accounting preparation" ? "approval" : "work",
                queue: terminal.includes(row.state)
                    ? "completed"
                    : actionable
                        ? "todo"
                        : "waiting",
                responsibleQueue: detail.responsibleQueue,
                nextAction: detail.nextAction,
                blocker: detail.blockedReason,
                dueAt: row.needed_date
                    ? Date.parse(row.needed_date + "T23:59:59Z")
                    : null,
                relationships: [
                    ...(r.own ? ["requested" as const] : []),
                    ...(r.assigned ? ["assigned" as const] : []),
                    ...(r.approval ? ["approval" as const] : []),
                    ...(r.team ? ["team" as const] : []),
                ],
            });
        }
        return items;
    }
    async function employeeRequestUpload(actor: WorkContext, id: string, purpose?: string) {
        const row = await getEmployeeRequest(actor, id), detail = await employeeRequestDetail(actor, id);
        const canEdit = detail.actions.includes("edit"), canWork = detail.relationship.assigned &&
            ["queued", "in_progress", "waiting_requester"].includes(row.state), canReply = detail.relationship.own && row.state === "waiting_requester";
        if (!canEdit && !canWork && !canReply)
            throw new BusinessRuleError(403, "REQUEST_EVIDENCE_LOCKED", "Files can be added while preparing the request or carrying out the assigned work.");
        const allowedPurpose = canEdit ? "request_evidence" : "fulfillment";
        const allowedPurposes = canEdit && row.category === "it" ? ["request_evidence", "it_request_evidence"] : [allowedPurpose];
        if (purpose && !allowedPurposes.includes(purpose))
            throw new BusinessRuleError(422, "EVIDENCE_PURPOSE_INVALID", `Use ${allowedPurpose.replaceAll("_", " ")} for this stage.`);
        return row;
    }
    async function prepareRequestOrderHandoff(actor: WorkContext, requestId: string, orderId: string, key: string, expectedVersion?: number, editing = false, rfqId?: string) {
        if (actor.beta)
            throw new BusinessRuleError(403, "BETA_CAPABILITY_UNAVAILABLE", "Purchasing execution is unavailable in beta.");
        actor = ports.requirePermission(actor, "commerce.purchase");
        const snap = await ports.serviceAuthoritySnapshot(actor.companyId), detail = await employeeRequestDetail(actor, requestId), row = detail.request;
        const quotation = rfqId
            ? await r.readPrepareRequestOrderHandoffQuotation(requestId, rfqId)
            : null;
        const activeQuotationCount = !editing ? Number(await r.readPrepareRequestOrderHandoffActiveQuotationCount(requestId)) : 0;
        if (!editing && ((rfqId && !quotation) || (!rfqId && activeQuotationCount > 0)))
            throw new BusinessRuleError(409, "REQUEST_RFQ_REQUIRED", "Continue the active supplier quotation review before creating this request's purchase order.");
        if (row.kind !== "purchase" ||
            JSON.parse(row.approval_json).decision !== "approved" ||
            (!editing && !detail.actions.includes("link-order")))
            throw new BusinessRuleError(403, "PURCHASE_REQUEST_REQUIRED", "An approved purchase request assigned to you and in progress is required for this handoff.");
        if (!editing && row.version !== expectedVersion)
            throw new BusinessRuleError(409, "VERSION_CONFLICT", "The purchase request changed. Reopen it before preparing the order.");
        const existingLink = editing
            ? await r.readPrepareRequestOrderHandoffExistingLink(requestId, orderId)
            : null;
        if (editing && row.purchase_order_id !== orderId && !existingLink)
            throw new BusinessRuleError(409, "ORDER_LINK_CONFLICT", "This order is linked to another request.");
        await ports.validateServiceScope(actor, row);
        return {
            row,
            actor: {
                ...actor,
                branchId: row.branch_id,
                departmentId: row.department_id,
            },
            steps: [
                ...snap(key),
                requestVersionGuard(key, row),
                ...(!editing
                    ? [
                        r.stagePrepareRequestOrderHandoffEffect(orderId, ports.clock.now(), row.id, row.company_id),
                        r.stagePrepareRequestOrderHandoffEffect2(row.id, orderId, rfqId ?? null, row.version, row.evidence_version, actor.userId, ports.clock.now()),
                        event({ ...row, version: row.version + 1 }, actor, "purchase-order-created", "Purchase order prepared from the approved request.", { orderId, reviewedVersion: row.version }),
                        ...(await requestNotice({ ...row, version: row.version + 1 }, actor, `${row.title} · Purchase order prepared`)),
                    ]
                    : []),
            ],
        };
    }
    async function requestQuotationSource(actor: WorkContext, id: string) {
        ports.requirePermission(actor, "commerce.purchase");
        const detail = await employeeRequestDetail(actor, id), row = detail.request;
        if (actor.beta || row.kind !== "purchase" || JSON.parse(row.approval_json).decision !== "approved" || !detail.actions.includes("link-order"))
            throw new BusinessRuleError(403, "PURCHASE_REQUEST_REQUIRED", "An approved purchase request assigned to you and in progress is required for quotations.");
        await ports.validateServiceScope(actor, row);
        const allocated = (await r.readRequestQuotationSourceAllocated(id)).results;
        const byOrdinal = new Map(allocated.map(x => [Number(x.ordinal), Number(x.allocated_milli)]));
        const items = detail.items.map((item, index) => { const requestedMilli = quantityMilli(item.quantity), allocatedMilli = byOrdinal.get(index) ?? 0; return { ...item, unit: item.unit || "each", requestedMilli, allocatedMilli, remainingMilli: Math.max(0, requestedMilli - allocatedMilli) }; });
        return { request: row, items, fullyAllocated: items.every(item => item.remainingMilli === 0) };
    }
    async function previewEmployeeRequest(actor: WorkContext, input: z.infer<typeof employeeRequestSchema>, id?: string) {
        if (!(await ports.companyMember(actor.companyId, actor.userId)))
            throw new BusinessRuleError(403, "MEMBERSHIP_REQUIRED", "Current company membership is required.");
        const current = id ? await getEmployeeRequest(actor, id) : null;
        if (current && (!requestOwn(actor, current) || !editable.includes(current.state)))
            throw new BusinessRuleError(403, "REQUEST_LOCKED", "Only an unsubmitted request can be previewed.");
        if (current && (current.kind !== input.kind || (current.parent_request_id ?? null) !== (input.parentRequestId ?? null)))
            throw new BusinessRuleError(422, "REQUEST_IDENTITY", "Request kind and original service job cannot change.");
        let requester = current?.requester_user_id ?? actor.userId;
        if (input.parentRequestId) {
            const parent = await getEmployeeRequest(actor, input.parentRequestId), access = await requestAccess(actor, parent);
            if (parent.kind !== "service" || input.kind === "service" || (!access.member && !access.own) || terminal.includes(parent.state))
                throw new BusinessRuleError(403, "RESOURCE_LINK_FORBIDDEN", "Open an active service job you handle or requested.");
            requester = parent.requester_user_id;
        }
        const scope = { company_id: actor.companyId, branch_id: input.branchId ?? actor.branchId, department_id: input.departmentId ?? actor.departmentId };
        await ports.validateServiceScope(actor, scope);
        if (!(await ports.serviceMember(scope, requester)))
            throw new BusinessRuleError(403, "REQUEST_SCOPE_FORBIDDEN", "Requester is outside this workspace.");
        const row = { ...scope, kind: input.kind, title: input.title, description: input.description, location: input.location, needed_date: input.neededDate || null, items_json: JSON.stringify(input.items), currency: input.currency, requester_user_id: requester, created_by: current?.created_by ?? actor.userId } as EmployeeRequest;
        try {
            const route = input.kind === "service" ? null : await submittedRoute(row);
            if (route)
                return ports.namedRoute(actor, route);
            const resolved = await ports.resolveServiceTeam(scope, input.category);
            return resolved ? { team: resolved.team.name, steps: [{ label: "Receive and assign", person: resolved.team.name }, { label: "Resolve and request confirmation", person: resolved.team.name }, { label: "Confirm outcome", person: "Requester" }], blockers: [] } : { steps: [], setupPath: "/settings?section=approvals&tab=templates#service-teams", blockers: [`Configure ${categoryLabel(input.category)} coverage for this location in Administration. Contact the Company service administrator.`] };
        }
        catch (error) {
            if (error instanceof BusinessRuleError && [409, 422].includes(error.status))
                return { steps: [], blockers: [error.message] };
            throw error;
        }
    }
    return { requestVersionGuard, requestOwn, canOverrideEmployeeRequest, readRequestForOverride, getEmployeeRequest, requestAttachments, employeeRequestDetail, saveEmployeeRequest, actOnEmployeeRequest, listEmployeeRequests, employeeRequestWork, employeeRequestUpload, prepareRequestOrderHandoff, requestQuotationSource, previewEmployeeRequest };
}
