import type {SqlStatement} from '../../runtime/public/runtime/types';
import { RequestPageCommerceOwnedFacts } from '../../commerce/public/records-request-page-owned-facts';
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { activeStaff } from "@/server/modules/operations/public/staff-policy";
import { inServiceWorkspace } from "@/server/modules/support/public/service-access";
import { holdingResources as nativeHoldingResources } from '../../runtime/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '../../identity_access/public/iam-read-snapshot';
import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '../../workflow/public/commands';
import { ApiError as NativeApiError } from '../../runtime/public/http';
import { createRequestPageApplication, type RequestPageRepository } from '../application/request-page';
import { BusinessRuleError } from '../../shared';
import { RequestPageOrganizationFacts } from '../../organization/public/records-request-page-facts';
function application(provided?: SqlDatabase) {
    let database = provided!;
    const repository: RequestPageRepository = { readRequestPageOptionsNodesResultcompanySettings(...args) { return RequestPageOrganizationFacts(database).readRequestPageOptionsNodesResultcompanySettings(...args); },
        readRequestPageOptionsNodesResultcompanySettings2(...args) { return RequestPageCommerceOwnedFacts(database).readRequestPageOptionsNodesResultcompanySettings2(...args); } };
    return createRequestPageApplication<SqlStatement>({ repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, commands: { replay: (...args) => nativeReplayCommand(database, ...args), commit: (...args) => nativeExecuteCommand(database, ...args) }, prepare: async (holding) => {
            if (!provided)
                database = (await nativeHoldingResources(holding)).write;
        }, production: process.env.NODE_ENV === 'production', batch: effects => database.batch(effects), policyRead: (key, read) => nativePolicyRead(database, key, read), policyNow: nativePolicyNow, activeStaff: (...args) => fromNative(() => activeStaff(database, ...args)), inServiceWorkspace: (...args) => inServiceWorkspace(...args) });
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
export async function requestPageOptions(db: SqlDatabase, actor: WorkContext) { return invoke(() => application(db).requestPageOptions(actor)); }
