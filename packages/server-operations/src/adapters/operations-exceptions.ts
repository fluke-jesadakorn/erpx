import { hasModuleAccess } from '@erpx/catalogue/module-access';
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import type { WorkItem } from "@erpx/feature-work_projection/lib/work-item";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { visibleSourceRecord } from "@erpx/server-records/public/source-record-policy";
import { holdingResources as nativeHoldingResources } from '../../../../packages/server-runtime/src/public/runtime';
import { ApiError as NativeApiError } from '@erpx/server-runtime/public/http';
import { type Row, type Exception } from '@erpx/server-operations/domain/operations-exceptions';
import { createOperationsExceptionsApplication, type OperationsExceptionsRepository } from '@erpx/server-operations/application/operations-exceptions';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { OperationsExceptionsIdentityAccessFacts } from '@erpx/server-identity_access/public/operations-operations-exceptions-facts';
const scope = "e.company_id=? AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)";
function application() {
    let database!: SqlDatabase;
    const repository: OperationsExceptionsRepository<SqlStatement> = {
        readOperationsWorkspaceCandidatespoliciespeople(value: (string | null)[]) { return database.prepare(`SELECT e.* FROM operations_exceptions e WHERE ${scope} AND e.source_kind<>'field_service_order' ORDER BY e.updated_at DESC LIMIT 200`).bind(...value).all<Exception>(); },
        readOperationsWorkspaceCandidatespoliciespeople2(companyId: string) { return database.prepare("SELECT * FROM operations_queue_policies WHERE company_id=? AND NOT (jsonb_typeof(source_kinds_json::jsonb)='array' AND source_kinds_json::jsonb<>'[]'::jsonb AND source_kinds_json::jsonb <@ '[\"field_service_order\"]'::jsonb) ORDER BY area,revision DESC").bind(companyId).all<Row>(); },
        readOperationsWorkspaceCandidatespoliciespeople3(...args) { return OperationsExceptionsIdentityAccessFacts(database).readOperationsWorkspaceCandidatespoliciespeople3(...args); },
        readOperationsExceptionDetailEffect(recordId: string) { return database.prepare("SELECT * FROM operations_exception_events WHERE exception_id=? ORDER BY version").bind(recordId).all<Row>(); } };
    return createOperationsExceptionsApplication({ repository, clock: { now: Date.now }, prepare: async (holding) => {
            database = (await nativeHoldingResources(holding)).write;
        }, requirePermission: (actor, permission) => fromNativeSync(() => requirePermission(actor, permission)), visibleSourceRecord: (...args) => fromNative(() => visibleSourceRecord(...args, database)) });
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
export async function operationsWorkspace(a: WorkContext) { return invoke(() => application().operationsWorkspace(a)); }
export async function operationsExceptionDetail(a: WorkContext, recordId: string) { return invoke(() => application().operationsExceptionDetail(a, recordId)); }
export async function operationsExceptionWork(a:WorkContext,nativeField?:()=>Promise<WorkItem[]>):Promise<WorkItem[]>{
 if(!hasModuleAccess(a.modules,'operations'))return [];
 if(!hasModuleAccess(a.modules,'field_service'))return invoke(()=>application().operationsExceptionWork(a));
 if(!nativeField)throw new NativeApiError(503,'NATIVE_FIELD_OPERATIONS_WORK_REQUIRED','The native Field operations work source must be connected.');
 // Legacy queries exclude Field rows. A failed native source propagates; it
 // never falls back to a stale SQL or cached Field queue.
 const [legacy,native]=await Promise.all([invoke(()=>application().operationsExceptionWork(a)),nativeField()]);
 return [...legacy,...native];
}
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
