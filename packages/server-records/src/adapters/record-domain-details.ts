import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { brandIdentityWorkspace } from "@erpx/server-brand/public/brand-identity";
import { agentWorkspace } from "@erpx/server-agent/public/agent-governance";
import { productionWorkspace } from "@erpx/server-manufacturing/public/production";
import { ecommerceWorkspace } from "@erpx/server-ecommerce/public/ecommerce";
import { learningWorkspace } from "@erpx/server-learning/public/learning";
import { benefitsWorkspace } from "@erpx/server-benefits/public/benefits";
import { talentWorkspace } from "@erpx/server-talent/public/talent";
import { holdingResources as nativeHoldingResources } from '../../../../packages/server-runtime/src/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '@erpx/server-identity_access/public/iam-read-snapshot';

import { ApiError as NativeApiError } from '@erpx/server-runtime/public/http';
import { type DomainRecordKind, isDomainRecordKind as pureisDomainRecordKind } from '@erpx/server-records/domain/record-domain-details';
import { createRecordDomainDetailsApplication, type RecordDomainDetailsRepository } from '@erpx/server-records/application/record-domain-details';
import { BusinessRuleError } from '@erpx/server-shared/index';
import type { SqlStatement } from '@erpx/server-runtime/public/runtime/types';
export { domainRecordKinds } from '@erpx/server-records/domain/record-domain-details';
export type { DomainRecordKind } from '@erpx/server-records/domain/record-domain-details';
function application(provided?: SqlDatabase) {
    let database = provided!;
    const repository: RecordDomainDetailsRepository<SqlStatement> = {};
    return createRecordDomainDetailsApplication<SqlStatement>({repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, prepare: async (holding) => {
            if (!provided)
                database = (await nativeHoldingResources(holding)).write;
        }, production: process.env.NODE_ENV === 'production', batch: effects => database.batch(effects), policyRead: (key, read) => nativePolicyRead(database, key, read), policyNow: nativePolicyNow, brandIdentityWorkspace: (...args) => fromNative(() => brandIdentityWorkspace(...args)), agentWorkspace: (...args) => fromNative(() => agentWorkspace(...args)), productionWorkspace: (...args) => fromNative(() => productionWorkspace(...args)), ecommerceWorkspace: (...args) => fromNative(() => ecommerceWorkspace(...args)), learningWorkspace: (...args) => fromNative(() => learningWorkspace(...args)), benefitsWorkspace: (...args) => fromNative(() => benefitsWorkspace(...args)), talentWorkspace: (...args) => fromNative(() => talentWorkspace(...args))});
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
