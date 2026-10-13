import { z } from "zod";
import type { WorkItem } from "@/lib/work_projection/work-item";
import { hasPermission } from "@/lib/identity_access/permissions";
import { hasModuleAccess } from "@/data/module-access";
import { samePerson } from "@/lib/identity_access/iam";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { AuthorizedSourceRecord } from "@/server/modules/records/public/source-record-policy";
import type { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import type { readSourceRecord } from "@/server/modules/records/public/source-record-policy";
import type { visibleSourceRecord } from "@/server/modules/records/public/source-record-policy";
import type { sourceRecordGuards } from "@/server/modules/records/public/source-record-policy";
import { reason, version, createSchema, actionSchema, type Row, type Exception } from '../domain/operations-exceptions';
import { BusinessRuleError, type Clock, type IdentifierSource } from '../../shared';
import type { OperationsCommands } from '../domain/commands';
type Tail<Value extends unknown[]> = Value extends [
    unknown,
    ...infer Rest
] ? Rest : never;
type PortResult<Value, Effect> = Value extends Promise<infer Result> ? Promise<PortResult<Result, Effect>> : Value extends {
    bind(...args: never[]): unknown;
    run(): unknown;
} ? Effect : Value extends (...args: never[]) => unknown ? Value : Value extends readonly unknown[] ? {
    [Index in keyof Value]: PortResult<Value[Index], Effect>;
} : Value extends object ? {
    [Key in keyof Value]: PortResult<Value[Key], Effect>;
} : Value;
export interface OperationsExceptionsRepository<Effect> {
    guard1(key: string, value: string, company_id: string, userId: string, permission: string, branch_id: string | null, department_id: string | null): Effect;
    stageReviewQueuePolicySteps(companyId: string): Effect;
    guard3(key: string, value: "policy-review", recordId: string, companyId: string, expectedVersion: number, value2: "published" | "draft", value3: string[]): Effect;
    guard4(key: string, value: "review-authority", assignmentId: string, companyId: string, userId: string): Effect;
    stageReviewQueuePolicyEffect(now: number, companyId: string, recordId: string): Effect;
    stageReviewQueuePolicyEffect2(status: "published" | "rejected" | "retired", userId: string, now: number, recordId: string): Effect;
    readOperationsWorkspaceCandidatespoliciespeople(value: (string | null)[]): Promise<{
        results: (Exception)[];
    }>;
    readOperationsWorkspaceCandidatespoliciespeople2(companyId: string): Promise<{
        results: (Row)[];
    }>;
    readOperationsWorkspaceCandidatespoliciespeople3(companyId: string, now: number, now2: number): Promise<{
        results: (Row)[];
    }>;
    readOperationsExceptionDetailEffect(recordId: string): Promise<{
        results: (Row)[];
    }>;
    readOpenOperationsExceptionPolicy(policyId: string, companyId: string, policyVersion: number): Promise<Row | null>;
    stageOpenOperationsExceptionEffect(policyId: string): Effect;
    guard13(key: string, value: "policy", policyId: string, companyId: string, policyVersion: number): Effect;
    stageOpenOperationsExceptionEffect2(recordId: string, companyId: string, branchId: string | null, departmentId: string | null, value: string, kind: "commerce_order" | "support_ticket" | "field_service_order" | "staff_task" | "pos_sale" | "pms_record" | "asset" | "maintenance_plan" | "maintenance_work", id: string, version: number, value2: string, value3: string, value4: string, title: string, issue: string, priority: "normal" | "high" | "critical", ownerId: string, dueAt: number, userId: string, now: number, now2: number): Effect;
    stageOpenOperationsExceptionEffect3(value: string, recordId: string, userId: string, issue: string, value2: string, now: number): Effect;
    stageOpenOperationsExceptionEffect4(value: string, companyId: string, ownerId: string, recordId: string, now: number): Effect;
    stageChangeOperationsExceptionSteps(recordId: string): Effect;
    guard18(key: string, value: "exception", recordId: string, expectedVersion: number, status: string, value2: (string | null)[]): Effect;
    stageChangeOperationsExceptionEffect(status: string, owner: string, value: string, value2: string | null, value3: string | null, value4: string | null, value5: string | null, now: number, recordId: string): Effect;
    stageChangeOperationsExceptionEffect2(value: string, recordId: string, value2: number, action: "reject" | "acknowledge" | "start" | "approve" | "close" | "reopen" | "cancel" | "assign" | "prioritize" | "escalate" | "resolve", userId: string, reason: string, status: string, value3: string, now: number): Effect;
    stageChangeOperationsExceptionEffect3(value: string, companyId: string, value2: string, recordId: string, value3: number, now: number): Effect;
}
export interface OperationsExceptionsPorts<Effect> {
    repository: OperationsExceptionsRepository<Effect>;
    clock: Clock;
    identifiers: IdentifierSource;
    commands: OperationsCommands<Effect>;
    prepare(holdingId: string): Promise<void>;
    production: boolean;
    batch(effects: Effect[]): Promise<unknown>;
    policyRead<Result>(key: string, read: () => Promise<Result>): Promise<Result>;
    policyNow(): number;
    requirePermission(...args: Parameters<typeof requirePermission>): PortResult<ReturnType<typeof requirePermission>, Effect>;
    readSourceRecord(...args: [
        a: WorkContext,
        reference: Parameters<typeof readSourceRecord>[1]
    ]): PortResult<ReturnType<typeof readSourceRecord>, Effect>;
    visibleSourceRecord(...args: [
        a: WorkContext,
        reference: Parameters<typeof visibleSourceRecord>[1]
    ]): PortResult<ReturnType<typeof visibleSourceRecord>, Effect>;
    sourceRecordGuards(...args: Tail<Parameters<typeof sourceRecordGuards>>): PortResult<ReturnType<typeof sourceRecordGuards>, Effect>;
}
export function createOperationsExceptionsApplication<Effect>(ports: OperationsExceptionsPorts<Effect>) {
    const r = ports.repository;
    const args = (a: WorkContext) => [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId];
    function access(a: WorkContext, write = false) { ports.requirePermission(a, write ? "records.write" : "records.read"); if (!hasModuleAccess(a.modules, "operations"))
        throw new BusinessRuleError(403, "MODULE_FORBIDDEN", "Operations is unavailable to the selected role."); }
    function recipient(key: string, scope: Pick<Exception, "company_id" | "branch_id" | "department_id">, userId: string, permission = "records.write") {
        return r.guard1(key, `recipient-${permission}-${userId}`, scope.company_id, userId, permission, scope.branch_id, scope.department_id);
    }
    async function reviewQueuePolicy(a: WorkContext, recordId: string, raw: unknown, key: string) {
        access(a, true);
        ports.requirePermission(a, "records.review");
        const input = z.object({ action: z.enum(["publish", "reject", "retire"]), expectedVersion: version, reason }).parse(raw);
        await ports.prepare(a.holdingId);
        const op = `operations.policy.${input.action}`;
        const prior = await ports.commands.replay(a, key, op, recordId, input);
        if (prior)
            return prior;
        const now = ports.clock.now(), status = input.action === "publish" ? "published" : input.action === "reject" ? "rejected" : "retired", steps: Effect[] = [
            r.stageReviewQueuePolicySteps(a.companyId),
            r.guard3(key, "policy-review", recordId, a.companyId, input.expectedVersion, input.action === "retire" ? "published" : "draft", (input.action !== "retire" ? [a.userId] : [])),
            r.guard4(key, "review-authority", a.assignmentId, a.companyId, a.userId),
        ];
        if (input.action === "publish")
            steps.push(r.stageReviewQueuePolicyEffect(now, a.companyId, recordId));
        steps.push(r.stageReviewQueuePolicyEffect2(status, a.userId, now, recordId));
        return ports.commands.commit(a, key, op, input, recordId, input.expectedVersion + 1, { id: recordId, status, version: input.expectedVersion + 1 }, steps);
    }
    async function operationsWorkspace(a: WorkContext) {
        access(a);
        await ports.prepare(a.holdingId);
        const [candidates, policies, people] = await Promise.all([
            r.readOperationsWorkspaceCandidatespoliciespeople(args(a)),
            r.readOperationsWorkspaceCandidatespoliciespeople2(a.companyId),
            r.readOperationsWorkspaceCandidatespoliciespeople3(a.companyId, ports.clock.now(), ports.clock.now()),
        ]);
        const exceptions: (Exception & {
            source: AuthorizedSourceRecord;
        })[] = [];
        for (const row of candidates.results) {
            const source = await ports.visibleSourceRecord(a, { kind: row.source_kind, id: row.source_id });
            if (source)
                exceptions.push({ ...row, source });
        }
        const active = exceptions.filter(row => !["closed", "cancelled"].includes(row.status));
        return { exceptions, policies: policies.results, people: people.results, actorId: a.userId, canWrite: hasPermission(a, "records.write"), canReview: hasPermission(a, "records.review"), summary: { active: active.length, awaitingReview: active.filter(row => row.status === "awaiting_review").length, escalated: active.filter(row => row.status === "escalated").length, overdue: active.filter(row => row.due_at < ports.clock.now()).length, byArea: Object.fromEntries([...new Set(active.map(row => String(row.area)))].sort().map(area => [area, active.filter(row => row.area === area).length])) } };
    }
    async function operationsExceptionDetail(a: WorkContext, recordId: string) {
        const data = await operationsWorkspace(a), row = data.exceptions.find(item => item.id === recordId);
        if (!row)
            throw new BusinessRuleError(404, "OPERATIONS_EXCEPTION_NOT_FOUND", "Exception or source is unavailable in this scope.");
        await ports.prepare(a.holdingId);
        return { ...data, row, history: (await r.readOperationsExceptionDetailEffect(recordId)).results };
    }
    async function openOperationsException(a: WorkContext, raw: unknown, key: string) {
        access(a, true);
        const input = createSchema.parse(raw);
        await ports.prepare(a.holdingId);
        const recordId = `operations-exception-${key}`;
        const op = "operations.exception.open";
        const prior = await ports.commands.replay(a, key, op, recordId, input);
        if (prior) {
            await operationsExceptionDetail(a, recordId);
            return prior;
        }
        const source = await ports.readSourceRecord(a, input.source), policy = await r.readOpenOperationsExceptionPolicy(input.policyId, a.companyId, input.policyVersion);
        if (!policy || !(JSON.parse(String(policy.source_kinds_json)) as string[]).includes(source.kind))
            throw new BusinessRuleError(409, "OPERATIONS_POLICY_UNAVAILABLE", "Choose a published policy that permits this source kind.");
        const now = ports.clock.now(), dueAt = now + Number(policy.target_minutes) * 60000, snapshot = { id: policy.id, area: policy.area, revision: policy.revision, version: policy.version, targetMinutes: policy.target_minutes, sourceKinds: JSON.parse(String(policy.source_kinds_json)) }, recordScope = { company_id: source.companyId, branch_id: source.branchId, department_id: source.departmentId };
        return ports.commands.commit(a, key, op, input, recordId, 1, { id: recordId, status: "open", version: 1 }, [
            ...await ports.sourceRecordGuards(a, key, source),
            r.stageOpenOperationsExceptionEffect(input.policyId),
            r.guard13(key, "policy", input.policyId, a.companyId, input.policyVersion),
            recipient(key, recordScope, input.ownerId),
            r.stageOpenOperationsExceptionEffect2(recordId, a.companyId, source.branchId, source.departmentId, String(policy.area), source.kind, source.id, source.version, JSON.stringify(source), String(policy.id), JSON.stringify(snapshot), input.title, input.issue, input.priority, input.ownerId, dueAt, a.userId, now, now),
            r.stageOpenOperationsExceptionEffect3(`operations-event-${key}`, recordId, a.userId, input.issue, JSON.stringify({ source, policy: snapshot, ownerId: input.ownerId }), now),
            r.stageOpenOperationsExceptionEffect4(`operations-notice-${key}`, a.companyId, input.ownerId, recordId, now),
        ], { companyId: a.companyId, branchId: source.branchId, departmentId: source.departmentId });
    }
    async function changeOperationsException(a: WorkContext, recordId: string, raw: unknown, key: string) {
        access(a, true);
        const input = actionSchema.parse(raw);
        await ports.prepare(a.holdingId);
        const op = `operations.exception.${input.action}`;
        const prior = await ports.commands.replay(a, key, op, recordId, input);
        if (prior) {
            await operationsExceptionDetail(a, recordId);
            return prior;
        }
        const { row } = await operationsExceptionDetail(a, recordId), manage = ["assign", "prioritize", "reject", "approve", "reopen", "cancel"].includes(input.action);
        if (manage)
            ports.requirePermission(a, "records.review");
        else if (row.owner_id !== a.userId)
            throw new BusinessRuleError(403, "OPERATIONS_OWNER_REQUIRED", "The assigned owner performs this step.");
        const allowed: Record<string, string[]> = { acknowledge: ["open"], start: ["acknowledged", "escalated"], resolve: ["in_progress", "escalated"], approve: ["awaiting_review"], reject: ["awaiting_review"], close: ["resolved"], reopen: ["closed"], assign: ["open", "acknowledged", "in_progress", "escalated", "awaiting_review"], prioritize: ["open", "acknowledged", "in_progress", "escalated"], escalate: ["open", "acknowledged", "in_progress"], cancel: ["open", "acknowledged", "in_progress", "escalated", "awaiting_review"] };
        if (!allowed[input.action].includes(row.status))
            throw new BusinessRuleError(409, "OPERATIONS_STATE_CHANGED", "Review the current exception state.");
        if (["approve", "reject"].includes(input.action) && (samePerson(a, row.owner_id) || samePerson(a, row.resolved_by)))
            throw new BusinessRuleError(403, "OPERATIONS_INDEPENDENT_REVIEW", "Another eligible reviewer must decide on this resolution.");
        const now = ports.clock.now(), status = input.action === "assign" && row.status === "awaiting_review" ? "in_progress" : ({ acknowledge: "acknowledged", start: "in_progress", resolve: "awaiting_review", approve: "resolved", reject: "in_progress", close: "closed", reopen: "in_progress", escalate: "escalated", cancel: "cancelled" } as Record<string, string>)[input.action] ?? row.status, steps: Effect[] = [
            r.stageChangeOperationsExceptionSteps(recordId),
            r.guard18(key, "exception", recordId, input.expectedVersion, row.status, args(a)),
            ...await ports.sourceRecordGuards(a, key, row.source), recipient(key, row, a.userId, manage ? "records.review" : "records.write"),
        ];
        if (input.action === "assign")
            steps.push(recipient(key, row, input.ownerId));
        if (input.action === "escalate")
            steps.push(recipient(key, row, input.escalationOwnerId, "records.review"));
        const reset = ["reject", "reopen", "assign"].includes(input.action), owner = input.action === "assign" ? input.ownerId : row.owner_id;
        steps.push(r.stageChangeOperationsExceptionEffect(status, owner, input.action === "prioritize" ? input.priority : row.priority, input.action === "escalate" ? input.escalationOwnerId : row.escalation_owner_id, input.action === "resolve" ? input.resolution : reset ? null : row.resolution, input.action === "resolve" ? a.userId : reset ? null : row.resolved_by, input.action === "approve" ? a.userId : reset ? null : row.reviewed_by, now, recordId), r.stageChangeOperationsExceptionEffect2(`operations-event-${key}`, recordId, input.expectedVersion + 1, input.action, a.userId, input.reason, status, JSON.stringify(input), now), r.stageChangeOperationsExceptionEffect3(`operations-notice-${key}`, a.companyId, input.action === "escalate" ? input.escalationOwnerId : owner, recordId, input.expectedVersion + 1, now));
        return ports.commands.commit(a, key, op, input, recordId, input.expectedVersion + 1, { id: recordId, status, version: input.expectedVersion + 1 }, steps, { companyId: a.companyId, branchId: row.branch_id, departmentId: row.department_id });
    }
    async function operationsExceptionWork(a: WorkContext): Promise<WorkItem[]> {
        if (!hasPermission(a, "records.read") || !hasModuleAccess(a.modules, "operations"))
            return [];
        const data = await operationsWorkspace(a);
        return data.exceptions.map(row => {
            const terminal = ["closed", "cancelled"].includes(row.status), review = row.status === "awaiting_review", canAct = !terminal && (review ? data.canReview && !samePerson(a, row.resolved_by) && !samePerson(a, row.owner_id) : row.owner_id === a.userId && data.canWrite);
            return { id: row.id, type: "operational_exception", title: row.title, detail: row.issue, href: `/operations/${row.id}`, source: row.source, status: row.status, updatedAt: row.updated_at, dueAt: row.due_at, canAct, queue: terminal ? "completed" : canAct ? "todo" : "waiting", responsibleQueue: review ? "Independent Operations reviewer" : "Assigned exception owner", nextAction: review ? "Review the proposed resolution" : row.status === "open" ? "Acknowledge the exception" : row.status === "resolved" ? "Close the reviewed exception" : "Record progress and resolve the exception", relationships: [row.owner_id === a.userId ? "assigned" : row.created_by === a.userId ? "requested" : review ? "approval" : "team"] };
        });
    }
    return { reviewQueuePolicy, operationsWorkspace, operationsExceptionDetail, openOperationsException, changeOperationsException, operationsExceptionWork };
}
