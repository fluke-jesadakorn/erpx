import { aiRequestQuotaLock } from "@erpx/agent-server/public/ai-quota";
import { captureJobAuthority, requireJobAuthority } from "@erpx/identity-access-server/public/iam-jobs";
import { iamMutationGuards } from "@erpx/identity-access-server/public/iam-store";
import { sha256 as aiSessionHash } from "@erpx/platform-runtime-server/public/crypto";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { z } from "zod";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { BetaEnvironment } from "@erpx/manage-erpx-server/public/beta-config";
import { resolveAiProvider, aiSnapshotGuard, aiUsageStatement } from "@erpx/agent-server/public/ai-providers";
import { generateProviderJson, withAiGatewayScope } from "@erpx/agent-server/public/ai-transport";
import {employeeRequestDetail} from '@erpx/records-server/public/employee-requests-read';
import {requestVersionGuard} from '@erpx/records-server/public/employee-requests';
import { serviceAuthoritySnapshot, validateServiceScope } from "@erpx/support-server/public/service-access";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import { requestAiUsageCountSql } from './read-tail-count-sql';
import { existsGuardSql } from './read-tail-exists-sql';
import { assertProductionModuleEnabled, productionFeatureGuard } from "@erpx/pms-server/public/feature-availability";
import { generateThaiHotelMockAi, isThaiHotelAiMockEnabled } from "@erpx/platform-runtime-server/public/simulation-only-adapters";
import { holdingResources as nativeHoldingResources } from '../../../../../platform/runtime/server/src/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '@erpx/identity-access-server/public/iam-read-snapshot';
import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '@erpx/workflow-server/public/commands';
import { ApiError as NativeApiError } from '@erpx/platform-runtime-server/public/http';
import { requestAiInput, type Proposal } from '@erpx/records-server/domain/request-ai';
import { createRequestAiApplication, type RequestAiRepository } from '@erpx/records-server/application/request-ai';
import { BusinessRuleError } from '@erpx/platform-shared-server/index';
import type { SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { recordsAiQuota } from '@erpx/agent-server/public/records-ai-quota';
import { requestAiProposalProjection, requestAiProposalProjectionSql } from './ai-usage-projection';
import { RequestAiIdentityAccessFacts } from '@erpx/identity-access-server/public/records-request-ai-facts';
import { RequestAiWorkflowFacts } from '@erpx/workflow-server/public/records-request-ai-facts';
import { nativeRecordsStatement } from '@erpx/records-ui/adapters/server/native-records-command';
export { requestAiInput } from '@erpx/records-server/domain/request-ai';
export { requestAiResult } from '@erpx/records-server/domain/request-ai';
export type { RequestAiResult } from '@erpx/records-server/domain/request-ai';
function application(provided?: SqlDatabase, actor?: WorkContext) {
    let database = provided!;
    const repository: RequestAiRepository<SqlStatement> = { readRequestAiStatusProposal(id: string, companyId: string) {
            return database.prepare(`${requestAiProposalProjectionSql} WHERE request_id=? AND company_id=? ORDER BY created_at DESC LIMIT 1`)
                .bind(id, companyId)
                .first<Proposal>();
        },
        readQueueRequestAiOld(companyId: string, id: string, version: number, providerJson: string) {
            return database.prepare(`${requestAiProposalProjectionSql} WHERE company_id=? AND request_id=? AND source_version=? AND provider_json=?`)
                .bind(companyId, id, version, providerJson)
                .first<Proposal>();
        },
        readQueueRequestAiQuota(args, companyId) { return recordsAiQuota(database).read(args, companyId); },
        stageQueueRequestAiStatement(value: string, now: number, proposalId: string) {
            return nativeRecordsStatement('records_stmt_1', { companyId: actor?.companyId, actor, now: Date.now() })
                .bind(value, now, proposalId);
        },
        stageQueueRequestAiStatement2(proposalId: string, companyId: string, id: string, version: number, providerJson: string, value: string, now: number, now2: number) {
            return nativeRecordsStatement('records_stmt_2', { companyId: actor?.companyId, actor, now: Date.now() })
                .bind(proposalId, companyId, id, version, providerJson, value, now, now2);
        },
        guard6(key: string, value: "request-ai-quota", args: string[], companyId: string) {
            return recordsAiQuota(database).guard(key, value, args, companyId);
        },
        guard7(key: string, value: "request-ai-capacity", companyId: string, now: number, userId: string, value2: number) {
            const quota = requestAiUsageCountSql("company_id=? AND state='reserved' AND lease_until>?");
            const rate = requestAiUsageCountSql('actor_id=? AND created_at>?');
            return assertion(database, key, value, `${quota}<2 AND ${rate}<6`, [companyId, now, userId, value2]);
        },
        stageQueueRequestAiEffect(value: number, proposalId: string) {
            return nativeRecordsStatement('records_stmt_3', { companyId: actor?.companyId, actor, now: Date.now() })
                .bind(value, proposalId);
        },
        stageQueueRequestAiEffect2(proposalId: string, companyId: string, userId: string, month: string, value: number, now: number) {
            return nativeRecordsStatement('records_stmt_4', { companyId: actor?.companyId, actor, now: Date.now() })
                .bind(proposalId, companyId, userId, month, value, now);
        },
        readRunRequestAiClaimed(token: string, value: number, now: number, id: string, now2: number) {
            return nativeRecordsStatement('records_stmt_5', { companyId: actor?.companyId, actor, now: Date.now() })
                .bind(token, value, now, id, now2)
                .first<Proposal>();
        },
        readRunRequestAiGrant(...args) { return RequestAiIdentityAccessFacts(database).readRunRequestAiGrant(...args); },
        stageRunRequestAiEffect(...args) { return RequestAiWorkflowFacts(database).stageRunRequestAiEffect(...args); },
        guard13(...args) { return RequestAiIdentityAccessFacts(database).guard13(...args); },
        guard14(command: string, value: "request-ai-lease", id: string, token: string) {
            return assertion(database, command, value, existsGuardSql('request_ai_proposals', "id=? AND state='running' AND lease_token=? AND claim_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint)"), [id, token]);
        },
        stageRunRequestAiEffect2(value: string, now: number, id: string, token: string) {
            return nativeRecordsStatement('records_stmt_6', { companyId: actor?.companyId, actor, now: Date.now() })
                .bind(value, now, id, token);
        },
        stageRunRequestAiEffect3(inputTokens: number, outputTokens: number, id: string) {
            return nativeRecordsStatement('records_stmt_7', { companyId: actor?.companyId, actor, now: Date.now() })
                .bind(inputTokens, outputTokens, id);
        },
        stageRunRequestAiEffect4(...args) { return RequestAiWorkflowFacts(database).stageRunRequestAiEffect4(...args); },
        readRunRequestAiChanged(value: "failed" | "stale", message: string, now: number, id: string, token: string) {
            return nativeRecordsStatement('records_stmt_8', { companyId: actor?.companyId, actor, now: Date.now() })
                .bind(value, message, now, id, token)
                .first();
        },
        stageRunRequestAiEffect5(id: string) {
            return nativeRecordsStatement('records_stmt_9', { companyId: actor?.companyId, actor, now: Date.now() })
                .bind(id)
                .run();
        },
        readDrainRequestAiRows(now: number, value: string | null, value2: string | null) {
            return database.prepare(requestAiProposalProjection('id', " WHERE (state='queued' OR (state='running' AND claim_until<?)) AND attempts<3 AND (?::text IS NULL OR request_id=?) ORDER BY created_at LIMIT 2"))
                .bind(now, value, value2)
                .all<{
                id: string;
            }>();
        },
        stageDrainRequestAiEffect(now: number) {
            return nativeRecordsStatement('records_stmt_10', { companyId: actor?.companyId, actor, now: Date.now() })
                .bind(now)
                .run();
        },
        stageDrainRequestAiEffect2() {
            return nativeRecordsStatement('records_stmt_11', { companyId: actor?.companyId, actor, now: Date.now() })
                .run();
        } };
    return createRequestAiApplication({ repository, runEffect: effect => effect.run(), clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, commands: { replay: (...args) => nativeReplayCommand(database, ...args), commit: (...args) => nativeExecuteCommand(database, ...args) }, prepare: async (holding) => {
            if (!provided)
                database = (await nativeHoldingResources(holding)).write;
        }, production: process.env.NODE_ENV === 'production', batch: effects => database.batch(effects), policyRead: (key, read) => nativePolicyRead(database, key, read), policyNow: nativePolicyNow, aiRequestQuotaLock: (...args) => aiRequestQuotaLock(database, ...args), captureJobAuthority: (...args) => fromNative(() => captureJobAuthority(database, ...args)), requireJobAuthority: (...args) => fromNativeSync(() => requireJobAuthority(...args)), iamMutationGuards: (...args) => iamMutationGuards(database, ...args), aiSessionHash: (...args) => fromNative(() => aiSessionHash(...args)), resolveAiProvider: (...args) => fromNative(() => resolveAiProvider(database, ...args)), aiSnapshotGuard: (...args) => aiSnapshotGuard(database, ...args), aiUsageStatement: (...args) => aiUsageStatement(database, ...args), generateProviderJson: (...args) => fromNative(() => generateProviderJson(...args)), withAiGatewayScope: (...args) => withAiGatewayScope(...args), employeeRequestDetail: (...args) => fromNative(() => employeeRequestDetail(database, ...args)), requestVersionGuard: (...args) => requestVersionGuard(database, ...args), serviceAuthoritySnapshot: (...args) => fromNative(() => serviceAuthoritySnapshot(database, ...args)), validateServiceScope: (...args) => fromNative(() => validateServiceScope(database, ...args)), assertProductionModuleEnabled: (...args) => fromNative(() => assertProductionModuleEnabled(database, ...args)), productionFeatureGuard: (...args) => productionFeatureGuard(database, ...args), generateThaiHotelMockAi: (endpoint, messages) => fromNative(() => generateThaiHotelMockAi(endpoint, messages)), isThaiHotelAiMockEnabled: (...args) => isThaiHotelAiMockEnabled(...args) });
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
export async function requestAiStatus(db: SqlDatabase, actor: WorkContext, env: BetaEnvironment, id: string) { return invoke(() => application(db, actor).requestAiStatus(actor, env, id)); }
export async function queueRequestAi(db: SqlDatabase, actor: WorkContext, env: BetaEnvironment, id: string, input: z.infer<typeof requestAiInput>, key: string) { return invoke(() => application(db, actor).queueRequestAi(actor, env, id, input, key)); }
export async function runRequestAi(db: SqlDatabase, env: BetaEnvironment, id: string, generate?: typeof generateProviderJson) { return invoke(() => application(db).runRequestAi(env, id, generate)); }
export async function drainRequestAi(db: SqlDatabase, env: BetaEnvironment, requestId?: string) { return invoke(() => application(db).drainRequestAi(env, requestId)); }
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
