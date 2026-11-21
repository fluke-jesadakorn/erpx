import type { WorkItem } from "@erpx/feature-work_projection/lib/work-item";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { hasModuleAccess } from "@erpx/catalogue/module-access";
import { samePerson } from "@erpx/feature-identity_access/lib/iam";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { AuthorizedSourceRecord } from "@erpx/server-records/public/source-record-policy";
import type { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import type { visibleSourceRecord } from "@erpx/server-records/public/source-record-policy";
import { type Row, type Exception } from '@erpx/server-operations/domain/operations-exceptions';
import { BusinessRuleError, type Clock } from '@erpx/server-shared/index';
type PortResult<Value, Effect> = Value extends Promise<infer Result> ? Promise<PortResult<Result, Effect>> : Value extends {
    bind(...args: never[]): unknown;
    run(): unknown;
} ? Effect : Value extends (...args: never[]) => unknown ? Value : Value extends readonly unknown[] ? {
    [Index in keyof Value]: PortResult<Value[Index], Effect>;
} : Value extends object ? {
    [Key in keyof Value]: PortResult<Value[Key], Effect>;
} : Value;
export interface OperationsExceptionsRepository<Effect> {
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
}
export interface OperationsExceptionsPorts<Effect> {
    repository: OperationsExceptionsRepository<Effect>;
    clock: Clock;
    prepare(holdingId: string): Promise<void>;
    requirePermission(...args: Parameters<typeof requirePermission>): PortResult<ReturnType<typeof requirePermission>, Effect>;
    visibleSourceRecord(...args: [
        a: WorkContext,
        reference: Parameters<typeof visibleSourceRecord>[1]
    ]): PortResult<ReturnType<typeof visibleSourceRecord>, Effect>;
}
export function createOperationsExceptionsApplication<Effect>(ports: OperationsExceptionsPorts<Effect>) {
    const r = ports.repository;
    const args = (a: WorkContext) => [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId];
    function access(a: WorkContext) { ports.requirePermission(a, "records.read"); if (!hasModuleAccess(a.modules, "operations"))
        throw new BusinessRuleError(403, "MODULE_FORBIDDEN", "Operations is unavailable to the selected role."); }
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
    async function operationsExceptionWork(a: WorkContext): Promise<WorkItem[]> {
        if (!hasPermission(a, "records.read") || !hasModuleAccess(a.modules, "operations"))
            return [];
        const data = await operationsWorkspace(a);
        return data.exceptions.map(row => {
            const terminal = ["closed", "cancelled"].includes(row.status), review = row.status === "awaiting_review", canAct = !terminal && (review ? data.canReview && !samePerson(a, row.resolved_by) && !samePerson(a, row.owner_id) : row.owner_id === a.userId && data.canWrite);
            return { id: row.id, type: "operational_exception", title: row.title, detail: row.issue, href: `/operations/${row.id}`, source: row.source, status: row.status, updatedAt: row.updated_at, dueAt: row.due_at, canAct, queue: terminal ? "completed" : canAct ? "todo" : "waiting", responsibleQueue: review ? "Independent Operations reviewer" : "Assigned exception owner", nextAction: review ? "Review the proposed resolution" : row.status === "open" ? "Acknowledge the exception" : row.status === "resolved" ? "Close the reviewed exception" : "Record progress and resolve the exception", relationships: [row.owner_id === a.userId ? "assigned" : row.created_by === a.userId ? "requested" : review ? "approval" : "team"] };
        });
    }
    return { operationsWorkspace, operationsExceptionDetail, operationsExceptionWork };
}
