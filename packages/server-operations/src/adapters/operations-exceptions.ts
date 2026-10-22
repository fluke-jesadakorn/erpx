import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import type { WorkItem } from "@erpx/feature-work_projection/lib/work-item";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { readSourceRecord, visibleSourceRecord, sourceRecordGuards } from "@erpx/server-records/public/source-record-policy";
import { holdingResources as nativeHoldingResources } from '../../../../packages/server-runtime/src/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '@erpx/server-identity_access/public/iam-read-snapshot';
import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '@erpx/server-workflow/public/commands';
import { ApiError as NativeApiError } from '@erpx/server-runtime/public/http';
import { type Row, type Exception } from '@erpx/server-operations/domain/operations-exceptions';
import { createOperationsExceptionsApplication, type OperationsExceptionsRepository } from '@erpx/server-operations/application/operations-exceptions';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { OperationsExceptionsIdentityAccessFacts } from '@erpx/server-identity_access/public/operations-operations-exceptions-facts';
import { OperationsExceptionsOrganizationFacts } from '@erpx/server-organization/public/operations-operations-exceptions-facts';
import { OperationsExceptionsCollaborationFacts } from '@erpx/server-collaboration/public/operations-operations-exceptions-facts';
const scope = "e.company_id=? AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)";
function application(provided?: SqlDatabase) {
    let database = provided!;
    const repository: OperationsExceptionsRepository<SqlStatement> = { guard1(...args) { return OperationsExceptionsIdentityAccessFacts(database).guard1(...args); },
        stageReviewQueuePolicySteps(...args) { return OperationsExceptionsOrganizationFacts(database).stageReviewQueuePolicySteps(...args); },
        guard3(...args) { return OperationsExceptionsIdentityAccessFacts(database).guard3(...args); },
        guard4(...args) { return OperationsExceptionsIdentityAccessFacts(database).guard4(...args); },
        stageReviewQueuePolicyEffect(now: number, companyId: string, recordId: string) { return database.prepare("UPDATE operations_queue_policies SET status='retired',version=version+1,updated_at=? WHERE company_id=? AND area=(SELECT area FROM operations_queue_policies WHERE id=?) AND status='published'").bind(now, companyId, recordId); },
        stageReviewQueuePolicyEffect2(status: "published" | "rejected" | "retired", userId: string, now: number, recordId: string) { return database.prepare("UPDATE operations_queue_policies SET status=?,reviewed_by=?,version=version+1,updated_at=? WHERE id=?").bind(status, userId, now, recordId); },
        readOperationsWorkspaceCandidatespoliciespeople(value: (string | null)[]) { return database.prepare(`SELECT e.* FROM operations_exceptions e WHERE ${scope} ORDER BY e.updated_at DESC LIMIT 200`).bind(...value).all<Exception>(); },
        readOperationsWorkspaceCandidatespoliciespeople2(companyId: string) { return database.prepare("SELECT * FROM operations_queue_policies WHERE company_id=? ORDER BY area,revision DESC").bind(companyId).all<Row>(); },
        readOperationsWorkspaceCandidatespoliciespeople3(...args) { return OperationsExceptionsIdentityAccessFacts(database).readOperationsWorkspaceCandidatespoliciespeople3(...args); },
        readOperationsExceptionDetailEffect(recordId: string) { return database.prepare("SELECT * FROM operations_exception_events WHERE exception_id=? ORDER BY version").bind(recordId).all<Row>(); },
        readOpenOperationsExceptionPolicy(policyId: string, companyId: string, policyVersion: number) { return database.prepare("SELECT * FROM operations_queue_policies WHERE id=? AND company_id=? AND version=? AND status='published'").bind(policyId, companyId, policyVersion).first<Row>(); },
        stageOpenOperationsExceptionEffect(policyId: string) { return database.prepare("SELECT id FROM operations_queue_policies WHERE id=? FOR SHARE").bind(policyId); },
        guard13(key: string, value: "policy", policyId: string, companyId: string, policyVersion: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM operations_queue_policies WHERE id=? AND company_id=? AND version=? AND status='published')", [policyId, companyId, policyVersion]); },
        stageOpenOperationsExceptionEffect2(recordId: string, companyId: string, branchId: string | null, departmentId: string | null, value: string, kind: "commerce_order" | "support_ticket" | "field_service_order" | "staff_task" | "pos_sale" | "pms_record" | "asset" | "maintenance_plan" | "maintenance_work", id: string, version: number, value2: string, value3: string, value4: string, title: string, issue: string, priority: "normal" | "high" | "critical", ownerId: string, dueAt: number, userId: string, now: number, now2: number) {
            return database.prepare("INSERT INTO operations_exceptions(id,company_id,branch_id,department_id,area,source_kind,source_id,source_version,source_reference_json,policy_id,policy_snapshot_json,title,issue,priority,owner_id,due_at,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(recordId, companyId, branchId, departmentId, value, kind, id, version, value2, value3, value4, title, issue, priority, ownerId, dueAt, userId, now, now2);
        },
        stageOpenOperationsExceptionEffect3(value: string, recordId: string, userId: string, issue: string, value2: string, now: number) { return database.prepare("INSERT INTO operations_exception_events VALUES(?,?,1,'open',?,?,'open',?,?)").bind(value, recordId, userId, issue, value2, now); },
        stageOpenOperationsExceptionEffect4(...args) { return OperationsExceptionsCollaborationFacts(database).stageOpenOperationsExceptionEffect4(...args); },
        stageChangeOperationsExceptionSteps(recordId: string) { return database.prepare("SELECT id FROM operations_exceptions WHERE id=? FOR UPDATE").bind(recordId); },
        guard18(key: string, value: "exception", recordId: string, expectedVersion: number, status: string, value2: (string | null)[]) { return assertion(database, key, value, `EXISTS(SELECT 1 FROM operations_exceptions e WHERE e.id=? AND e.version=? AND e.status=? AND ${scope})`, [recordId, expectedVersion, status, ...value2]); },
        stageChangeOperationsExceptionEffect(status: string, owner: string, value: string, value2: string | null, value3: string | null, value4: string | null, value5: string | null, now: number, recordId: string) {
            return database.prepare("UPDATE operations_exceptions SET status=?,owner_id=?,priority=?,escalation_owner_id=?,resolution=?,resolved_by=?,reviewed_by=?,version=version+1,updated_at=? WHERE id=?")
                .bind(status, owner, value, value2, value3, value4, value5, now, recordId);
        },
        stageChangeOperationsExceptionEffect2(value: string, recordId: string, value2: number, action: "reject" | "acknowledge" | "start" | "approve" | "close" | "reopen" | "cancel" | "assign" | "prioritize" | "escalate" | "resolve", userId: string, reason: string, status: string, value3: string, now: number) { return database.prepare("INSERT INTO operations_exception_events VALUES(?,?,?,?,?,?,?,?,?)").bind(value, recordId, value2, action, userId, reason, status, value3, now); },
        stageChangeOperationsExceptionEffect3(...args) { return OperationsExceptionsCollaborationFacts(database).stageChangeOperationsExceptionEffect3(...args); } };
    return createOperationsExceptionsApplication({ repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, commands: { replay: (...args) => nativeReplayCommand(database, ...args), commit: (...args) => nativeExecuteCommand(database, ...args) }, prepare: async (holding) => {
            if (!provided)
                database = (await nativeHoldingResources(holding)).write;
        }, production: process.env.NODE_ENV === 'production', batch: effects => database.batch(effects), policyRead: (key, read) => nativePolicyRead(database, key, read), policyNow: nativePolicyNow, requirePermission: (actor, permission) => fromNativeSync(() => requirePermission(actor, permission)), readSourceRecord: (...args) => fromNative(() => readSourceRecord(...args, database)), visibleSourceRecord: (...args) => fromNative(() => visibleSourceRecord(...args, database)), sourceRecordGuards: (...args) => fromNative(() => sourceRecordGuards(database, ...args)), });
}
async function fromNative<Result>(run: () => Promise<Result> | Result): Promise<Result> {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof NativeApiError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    }
}
async function invoke<Result>(run: () => Promise<Result>) {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new NativeApiError(error.status, error.code, error.message);
        throw error;
    }
}
export async function reviewQueuePolicy(a: WorkContext, recordId: string, raw: unknown, key: string) { return invoke(() => application().reviewQueuePolicy(a, recordId, raw, key)); }
export async function operationsWorkspace(a: WorkContext) { return invoke(() => application().operationsWorkspace(a)); }
export async function operationsExceptionDetail(a: WorkContext, recordId: string) { return invoke(() => application().operationsExceptionDetail(a, recordId)); }
export async function openOperationsException(a: WorkContext, raw: unknown, key: string) { return invoke(() => application().openOperationsException(a, raw, key)); }
export async function changeOperationsException(a: WorkContext, recordId: string, raw: unknown, key: string) { return invoke(() => application().changeOperationsException(a, recordId, raw, key)); }
export async function operationsExceptionWork(a: WorkContext): Promise<WorkItem[]> { return invoke(() => application().operationsExceptionWork(a)); }
export { draftQueuePolicy } from "@erpx/server-operations/adapters/queue-policy";
function fromNativeSync<Result>(run: () => Result): Result {
    try {
        return run();
    }
    catch (error) {
        if (error instanceof NativeApiError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    }
}
