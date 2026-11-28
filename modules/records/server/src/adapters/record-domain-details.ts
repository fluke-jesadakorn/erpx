import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { nativeBrandIdentityWorkspace } from "@erpx/brand-ui/lib/native-brand-identity";
import { agentWorkspace } from "@erpx/agent-server/public/agent-governance";
import { productionWorkspace } from "@erpx/manufacturing-server/public/production";
import { ecommerceWorkspace } from "@erpx/ecommerce-server/public/ecommerce";
import { nativeLearningWorkspacePage } from "@erpx/platform-workplace/lib/native-learning";
import { benefitsWorkspace } from "@erpx/benefits-server/public/benefits-native-read";
import { talentWorkspace } from "@erpx/talent-server/public/talent";
import { holdingResources as nativeHoldingResources } from '../../../../../platform/runtime/server/src/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '@erpx/identity-access-server/public/iam-read-snapshot';

import { ApiError as NativeApiError } from '@erpx/platform-runtime-server/public/http';
import { type DomainRecordKind, isDomainRecordKind as pureisDomainRecordKind } from '@erpx/records-server/domain/record-domain-details';
import { createRecordDomainDetailsApplication, type RecordDomainDetailsRepository } from '@erpx/records-server/application/record-domain-details';
import { BusinessRuleError } from '@erpx/platform-shared-server/index';
import type { SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
export { domainRecordKinds } from '@erpx/records-server/domain/record-domain-details';
export type { DomainRecordKind } from '@erpx/records-server/domain/record-domain-details';
function application(provided?: SqlDatabase) {
    let database = provided!;
    const repository: RecordDomainDetailsRepository<SqlStatement> = {};
    return createRecordDomainDetailsApplication<SqlStatement>({repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, prepare: async (holding) => {
            if (!provided)
                database = (await nativeHoldingResources(holding)).write;
        }, production: process.env.NODE_ENV === 'production', batch: effects => database.batch(effects), policyRead: (key, read) => nativePolicyRead(database, key, read), policyNow: nativePolicyNow, brandIdentityWorkspace: (...args) => fromNative(async () => { const [actor] = args, data = await nativeBrandIdentityWorkspace(actor, actor.holdingSlug); if (!data) throw new NativeApiError(503, 'NATIVE_BRAND_UNAVAILABLE', 'The native Brand identity reader is unavailable.'); return data; }), agentWorkspace: (...args) => fromNative(() => agentWorkspace(...args)), productionWorkspace: (...args) => fromNative(() => productionWorkspace(...args)), ecommerceWorkspace: (...args) => fromNative(() => ecommerceWorkspace(...args)), learningWorkspace: (...args) => fromNative(() => nativeLearningWorkspacePage(...args)), benefitsWorkspace: (...args) => fromNative(() => benefitsWorkspace(...args)), talentWorkspace: (...args) => fromNative(() => talentWorkspace(...args))});
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
