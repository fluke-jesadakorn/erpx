import { aiRequestQuotaLock } from "@/server/modules/agent/public/ai-quota";
import { captureJobAuthority, requireJobAuthority } from "@/server/modules/identity_access/public/iam-jobs";
import { iamMutationGuards } from "@/server/modules/identity_access/public/iam-store";
import { sha256 as aiSessionHash } from "@/server/modules/runtime/public/crypto";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { z } from "zod";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { BetaEnvironment } from "@/server/modules/manage_erpx/public/beta-config";
import { resolveAiProvider, aiSnapshotGuard, aiUsageStatement } from "@/server/modules/agent/public/ai-providers";
import { generateProviderJson, withAiGatewayScope } from "@/server/modules/agent/public/ai-transport";
import { employeeRequestDetail, requestVersionGuard } from "@/server/modules/records/public/employee-requests";
import { serviceAuthoritySnapshot, validateServiceScope } from "@/server/modules/support/public/service-access";
import { assertion } from "@/server/modules/workflow/public/commands";
import { assertProductionModuleEnabled, productionFeatureGuard } from "@/server/modules/pms/public/hospitality-rollout";
import { generateThaiHotelMockAi, isThaiHotelAiMockEnabled } from "@/server/modules/runtime/public/simulation-only-adapters";
import { holdingResources as nativeHoldingResources } from '../../runtime/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '../../identity_access/public/iam-read-snapshot';
import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '../../workflow/public/commands';
import { ApiError as NativeApiError } from '../../runtime/public/http';
import { requestAiInput, type Proposal } from '../domain/request-ai';
import { createRequestAiApplication, type RequestAiRepository } from '../application/request-ai';
import { BusinessRuleError } from '../../shared';
import type { SqlStatement } from '../../runtime/public/runtime/types';
import { recordsAiQuota } from '../../agent/public/records-ai-quota';
import { RequestAiIdentityAccessFacts } from '../../identity_access/public/records-request-ai-facts';
import { RequestAiWorkflowFacts } from '../../workflow/public/records-request-ai-facts';
export { requestAiInput } from '../domain/request-ai';
export { requestAiResult } from '../domain/request-ai';
export type { RequestAiResult } from '../domain/request-ai';
function application(provided?: SqlDatabase) {
    let database = provided!;
    const repository: RequestAiRepository<SqlStatement> = { readRequestAiStatusProposal(id: string, companyId: string) {
            return database.prepare("SELECT * FROM request_ai_proposals WHERE request_id=? AND company_id=? ORDER BY created_at DESC LIMIT 1")
                .bind(id, companyId)
                .first<Proposal>();
        },
        readQueueRequestAiOld(companyId: string, id: string, version: number, providerJson: string) {
            return database.prepare("SELECT * FROM request_ai_proposals WHERE company_id=? AND request_id=? AND source_version=? AND provider_json=?")
                .bind(companyId, id, version, providerJson)
                .first<Proposal>();
        },
        readQueueRequestAiQuota(args, companyId) { return recordsAiQuota(database).read(args, companyId); },
        stageQueueRequestAiStatement(value: string, now: number, proposalId: string) {
            return database.prepare("UPDATE request_ai_proposals SET state='queued',error=NULL,attempts=0,lease_token=NULL,claim_until=NULL,actor_json=?,updated_at=? WHERE id=? AND state IN ('failed','stale')")
                .bind(value, now, proposalId);
        },
        stageQueueRequestAiStatement2(proposalId: string, companyId: string, id: string, version: number, providerJson: string, value: string, now: number, now2: number) {
            return database.prepare("INSERT INTO request_ai_proposals(id,company_id,request_id,source_version,provider_json,actor_json,state,created_at,updated_at) VALUES(?,?,?,?,?,?,'queued',?,?)")
                .bind(proposalId, companyId, id, version, providerJson, value, now, now2);
        },
        guard6(key: string, value: "request-ai-quota", args: string[], companyId: string) {
            return recordsAiQuota(database).guard(key, value, args, companyId);
        },
        guard7(key: string, value: "request-ai-capacity", companyId: string, now: number, userId: string, value2: number) {
            return assertion(database, key, value, "(SELECT count(*) FROM request_ai_usage WHERE company_id=? AND state='reserved' AND lease_until>?)<2 AND (SELECT count(*) FROM request_ai_usage WHERE actor_id=? AND created_at>?)<6", [companyId, now, userId, value2]);
        },
        stageQueueRequestAiEffect(value: number, proposalId: string) {
            return database.prepare("UPDATE request_ai_usage SET state='reserved',lease_until=? WHERE id=?")
                .bind(value, proposalId);
        },
        stageQueueRequestAiEffect2(proposalId: string, companyId: string, userId: string, month: string, value: number, now: number) {
            return database.prepare("INSERT INTO request_ai_usage(id,company_id,actor_id,month,state,lease_until,created_at) VALUES(?,?,?,?,'reserved',?,?)")
                .bind(proposalId, companyId, userId, month, value, now);
        },
        readRunRequestAiClaimed(token: string, value: number, now: number, id: string, now2: number) {
            return database.prepare("UPDATE request_ai_proposals SET state='running',lease_token=?,claim_until=?,attempts=attempts+1,updated_at=? WHERE id=? AND attempts<3 AND (state='queued' OR (state='running' AND claim_until<?)) RETURNING *")
                .bind(token, value, now, id, now2)
                .first<Proposal>();
        },
        readRunRequestAiGrant(...args) { return RequestAiIdentityAccessFacts(database).readRunRequestAiGrant(...args); },
        stageRunRequestAiEffect(...args) { return RequestAiWorkflowFacts(database).stageRunRequestAiEffect(...args); },
        guard13(...args) { return RequestAiIdentityAccessFacts(database).guard13(...args); },
        guard14(command: string, value: "request-ai-lease", id: string, token: string) {
            return assertion(database, command, value, "EXISTS(SELECT 1 FROM request_ai_proposals WHERE id=? AND state='running' AND lease_token=? AND claim_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint))", [id, token]);
        },
        stageRunRequestAiEffect2(value: string, now: number, id: string, token: string) {
            return database.prepare("UPDATE request_ai_proposals SET state='ready',result_json=?,error=NULL,lease_token=NULL,claim_until=NULL,updated_at=? WHERE id=? AND lease_token=?")
                .bind(value, now, id, token);
        },
        stageRunRequestAiEffect3(inputTokens: number, outputTokens: number, id: string) {
            return database.prepare("UPDATE request_ai_usage SET state='succeeded',lease_until=0,input_tokens=?,output_tokens=? WHERE id=?")
                .bind(inputTokens, outputTokens, id);
        },
        stageRunRequestAiEffect4(...args) { return RequestAiWorkflowFacts(database).stageRunRequestAiEffect4(...args); },
        readRunRequestAiChanged(value: "failed" | "stale", message: string, now: number, id: string, token: string) {
            return database.prepare("UPDATE request_ai_proposals SET state=?,error=?,lease_token=NULL,claim_until=NULL,updated_at=? WHERE id=? AND state='running' AND lease_token=? RETURNING id")
                .bind(value, message, now, id, token)
                .first();
        },
        stageRunRequestAiEffect5(id: string) {
            return database.prepare("UPDATE request_ai_usage SET state='failed',lease_until=0 WHERE id=?")
                .bind(id)
                .run();
        },
        readDrainRequestAiRows(now: number, value: string | null, value2: string | null) {
            return database.prepare("SELECT id FROM request_ai_proposals WHERE (state='queued' OR (state='running' AND claim_until<?)) AND attempts<3 AND (?::text IS NULL OR request_id=?) ORDER BY created_at LIMIT 2")
                .bind(now, value, value2)
                .all<{
                id: string;
            }>();
        },
        stageDrainRequestAiEffect(now: number) {
            return database.prepare("UPDATE request_ai_proposals SET state='failed',error='Preparation was interrupted. Retry or continue manually.',lease_token=NULL,claim_until=NULL WHERE state='running' AND claim_until<? AND attempts>=3")
                .bind(now)
                .run();
        },
        stageDrainRequestAiEffect2() {
            return database.prepare("UPDATE request_ai_usage SET state='failed',lease_until=0 WHERE state='reserved' AND id IN (SELECT id FROM request_ai_proposals WHERE state IN ('failed','stale'))")
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
export async function requestAiStatus(db: SqlDatabase, actor: WorkContext, env: BetaEnvironment, id: string) { return invoke(() => application(db).requestAiStatus(actor, env, id)); }
export async function queueRequestAi(db: SqlDatabase, actor: WorkContext, env: BetaEnvironment, id: string, input: z.infer<typeof requestAiInput>, key: string) { return invoke(() => application(db).queueRequestAi(actor, env, id, input, key)); }
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
