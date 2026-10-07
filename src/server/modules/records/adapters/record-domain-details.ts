import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { brandIdentityWorkspace } from "@/server/modules/brand/public/brand-identity";
import { agentWorkspace } from "@/server/modules/agent/public/agent-governance";
import { productionWorkspace } from "@/server/modules/manufacturing/public/production";
import { ecommerceWorkspace } from "@/server/modules/ecommerce/public/ecommerce";
import { learningWorkspace } from "@/server/modules/learning/public/learning";
import { benefitsWorkspace } from "@/server/modules/benefits/public/benefits";
import { talentWorkspace } from "@/server/modules/talent/public/talent";
import { holdingResources as nativeHoldingResources } from '../../runtime/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '../../identity_access/public/iam-read-snapshot';
import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '../../workflow/public/commands';
import { ApiError as NativeApiError } from '../../runtime/public/http';
import { type DomainRecordKind, isDomainRecordKind as pureisDomainRecordKind } from '../domain/record-domain-details';
import { createRecordDomainDetailsApplication, type RecordDomainDetailsRepository } from '../application/record-domain-details';
import { BusinessRuleError } from '../../shared';
import type { SqlStatement } from '../../runtime/public/runtime/types';
export { domainRecordKinds } from '../domain/record-domain-details';
export type { DomainRecordKind } from '../domain/record-domain-details';
function application(provided?: SqlDatabase) {
    let database = provided!;
    const repository: RecordDomainDetailsRepository<SqlStatement> = {};
    return createRecordDomainDetailsApplication<SqlStatement>({ repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, commands: { replay: (...args) => nativeReplayCommand(database, ...args), commit: (...args) => nativeExecuteCommand(database, ...args) }, prepare: async (holding) => {
            if (!provided)
                database = (await nativeHoldingResources(holding)).write;
        }, production: process.env.NODE_ENV === 'production', batch: effects => database.batch(effects), policyRead: (key, read) => nativePolicyRead(database, key, read), policyNow: nativePolicyNow, brandIdentityWorkspace: (...args) => fromNative(() => brandIdentityWorkspace(...args)), agentWorkspace: (...args) => fromNative(() => agentWorkspace(...args)), productionWorkspace: (...args) => fromNative(() => productionWorkspace(...args)), ecommerceWorkspace: (...args) => fromNative(() => ecommerceWorkspace(...args)), learningWorkspace: (...args) => fromNative(() => learningWorkspace(...args)), benefitsWorkspace: (...args) => fromNative(() => benefitsWorkspace(...args)), talentWorkspace: (...args) => fromNative(() => talentWorkspace(...args)) });
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
function invokeSync<Result>(run: () => Result) {
    try {
        return run();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new NativeApiError(error.status, error.code, error.message);
        throw error;
    }
}
export function isDomainRecordKind(kind: string): kind is DomainRecordKind { return invokeSync(() => pureisDomainRecordKind(kind)); }
export async function recordDomainData(actor: WorkContext, kind: DomainRecordKind, id: string) { return invoke(() => application().recordDomainData(actor, kind, id)); }
