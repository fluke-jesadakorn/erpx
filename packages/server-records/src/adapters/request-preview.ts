import type {SqlStatement} from '@erpx/server-runtime/public/runtime/types';
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { ApprovalRoute } from "@erpx/server-operations/public/staff-policy";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { RequestPreview } from "@erpx/feature-work_projection/lib/request-preview";
import { holdingResources as nativeHoldingResources } from '../../../../packages/server-runtime/src/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '@erpx/server-identity_access/public/iam-read-snapshot';
import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '@erpx/server-workflow/public/commands';
import { ApiError as NativeApiError } from '@erpx/server-runtime/public/http';
import { createRequestPreviewApplication, type RequestPreviewRepository } from '@erpx/server-records/application/request-preview';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { RequestPreviewPeoplePayrollFacts } from '@erpx/server-people_payroll/public/records-request-preview-facts';
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
