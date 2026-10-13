import type {SqlStatement} from '../../runtime/public/runtime/types';
import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { ApprovalRoute } from "@/server/modules/operations/public/staff-policy";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import type { RequestPreview } from "@/lib/work_projection/request-preview";
import { holdingResources as nativeHoldingResources } from '../../runtime/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '../../identity_access/public/iam-read-snapshot';
import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '../../workflow/public/commands';
import { ApiError as NativeApiError } from '../../runtime/public/http';
import { createRequestPreviewApplication, type RequestPreviewRepository } from '../application/request-preview';
import { BusinessRuleError } from '../../shared';
import { RequestPreviewPeoplePayrollFacts } from '../../people_payroll/public/records-request-preview-facts';
function application(provided?: SqlDatabase) {
    let database = provided!;
    const repository: RequestPreviewRepository = { readNamedRoutePeople(...args) { return RequestPreviewPeoplePayrollFacts(database).readNamedRoutePeople(...args); } };
    return createRequestPreviewApplication<SqlStatement>({ repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, commands: { replay: (...args) => nativeReplayCommand(database, ...args), commit: (...args) => nativeExecuteCommand(database, ...args) }, prepare: async (holding) => {
            if (!provided)
                database = (await nativeHoldingResources(holding)).write;
        }, production: process.env.NODE_ENV === 'production', batch: effects => database.batch(effects), policyRead: (key, read) => nativePolicyRead(database, key, read), policyNow: nativePolicyNow, });
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
export async function namedRoute(db: SqlDatabase, actor: WorkContext, route: ApprovalRoute): Promise<RequestPreview> { return invoke(() => application(db).namedRoute(actor, route)); }
