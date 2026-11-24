import 'server-only';
import {withWorkSocialAiPhase} from '@erpx/server-collaboration/adapters/work-social-purchases';
import type { AiEndpoint as NativeAiEndpoint } from '@erpx/server-agent/public/ai-transport';
import { z } from 'zod';
import type { StaffActor } from '@erpx/server-operations/public/staff-policy';
import type { BetaEnvironment } from '@erpx/server-manage_erpx/public/beta-config';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { ApiError } from '@erpx/server-runtime/public/http';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { sha256 } from '@erpx/server-runtime/public/crypto';
import {sourceSnapshot,sourceGuard,readablePost} from '@erpx/server-collaboration/public/social-source';
import {socialAiDraftById} from './read-tail-sql';
import { staffPeople } from '@erpx/server-operations/public/staff-policy';
import {workContextGuards} from '@erpx/server-workflow/public/command-guards';
import { aiRequestQuotaLock } from '@erpx/server-agent/public/ai-quota';
import { resolveAiProvider, aiSnapshotGuard, aiUsageStatement } from '@erpx/server-agent/public/ai-providers';
import { generateProviderJson, withAiGatewayScope } from '@erpx/server-agent/public/ai-transport';
import { collaborationAiSources as agent } from '@erpx/server-agent/public/collaboration-ai-sources';
import { collaborationAiSources as workflow } from '@erpx/server-workflow/public/collaboration-ai-sources';
import { aiRequestSchema } from '@erpx/server-collaboration/domain/social-ai';
import { createSocialAiApplication, type SocialAiRepository } from '@erpx/server-collaboration/application/social-ai';
import { nativeOwnerWriteSystem } from '@erpx/server-collaboration/adapters/native-owner-write';
async function inward<T>(action: () => Promise<T>) {
    try {
        return await action();
    }
    catch (error) {
        if (error instanceof ApiError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    }
}
async function exposed<T>(action: () => Promise<T>) {
    try {
        return await action();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new ApiError(error.status, error.code, error.message);
        throw error;
    }
}
function application(db?: SqlDatabase, companyId = '') {
    const owner=db;
    const write = (kind: string, values: unknown[], key: string, now: number) =>
        nativeOwnerWriteSystem(db!, companyId, kind, values, key, now);
    const repository: SocialAiRepository<SqlStatement> = { async priorDraft(id: string, userId: string) {
            return db!
                .prepare(socialAiDraftById('actor_json,post_id,kind,source_hash,result_json,state,source_version,comment_ids_json,expires_at,provider_json'))
                .bind(id, userId)
                .first<{
                actor_json: string | null;
                post_id: string;
                kind: string;
                source_hash: string;
                result_json: string | null;
                state: string;
                source_version: number;
                comment_ids_json: string;
                expires_at: number;
                provider_json: string | null;
            }>();
        }, prepareDraft(id: string, postId: string, userId: string, kind: "summary" | "task", version: number, hash: string, value: string, value2: string, value3: string, now: number, value4: number) {
            return write('ai_draft_prepare', [id, postId, userId, kind, version, hash, value, value2, value3, now, value4], `ai-draft:${id}`, now);
        }, reserveUsage(id: string, companyId: string, userId: string, month: string, value: number, now: number) {
            return write('ai_usage_reserve', [id, companyId, userId, month, value, now], `ai-usage:${id}`, now);
        }, captureProvider(value: string, id: string) {
            return write('ai_draft_capture_provider', [value, id], `ai-draft-provider:${id}`, Date.now());
        }, captureActor(value: string, id: string) { return write('ai_draft_capture_actor', [value, id], `ai-draft-actor:${id}`, Date.now()); }, finishDraft(value: string, id: string) {
            return write('ai_draft_finish', [value, id], `ai-draft-finish:${id}`, Date.now());
        }, finishUsage(inputTokens: number, outputTokens: number, id: string) {
            return write('ai_usage_finish', [inputTokens, outputTokens, id], `ai-usage-finish:${id}`, Date.now());
        }, failDraft(id: string) {
            return write('ai_draft_fail', [id], `ai-draft-fail:${id}`, Date.now());
        }, failUsage(id: string) {
            return write('ai_usage_fail', [id], `ai-usage-fail:${id}`, Date.now());
        }, guardQuota:(...args)=>agent(db!).guardQuota(...args),clearReserveAssertions:(...args)=>workflow(db!).clearReserveAssertions(...args),clearFinishAssertions:(...args)=>workflow(db!).clearFinishAssertions(...args) };
    return createSocialAiApplication<SqlStatement>({ repository, clock: { now: () => Date.now() }, identifiers: { next: () => crypto.randomUUID() }, sha256, batch: effects => db!.batch(effects), writePhase:(actor,phase,prepare)=>inward(()=>withWorkSocialAiPhase(owner!,actor,phase,async transaction=>{const previous=db;db=transaction;try{return await transaction.batch(await prepare());}finally{db=previous;}})), sourceSnapshot: (...args) => inward(() => sourceSnapshot(db!, ...args)), sourceGuard: (...args) => sourceGuard(db!, ...args), readablePost: (...args) => inward(() => readablePost(db!, ...args)), staffPeople: (...args) => inward(() => staffPeople(db!, ...args)), workContextGuards: (...args) => workContextGuards(db!, ...args), aiRequestQuotaLock: (...args) => aiRequestQuotaLock(db!, ...args), resolveAiProvider: (...args) => inward(() => resolveAiProvider(db!, ...args)), aiSnapshotGuard: (...args) => aiSnapshotGuard(db!, ...args), aiUsageStatement: (...args) => aiUsageStatement(db!, ...args), generateProviderJson: (endpoint, messages, tokens) => inward(() => generateProviderJson(endpoint as NativeAiEndpoint, messages, tokens)), withAiGatewayScope: (endpoint, ...args) => withAiGatewayScope(endpoint as NativeAiEndpoint, ...args) });
}
export function callAi(env: BetaEnvironment, input: unknown) { return exposed(() => application().callAi(env, input)); }
export function createAiDraft(db: SqlDatabase, actor: StaffActor, postId: string, input: z.infer<typeof aiRequestSchema>, env: BetaEnvironment, provider = callAi, commandKey?: string) { return exposed(() => application(db, actor.companyId).createAiDraft(actor, postId, input, env, provider, commandKey, provider === callAi)); }
export { aiRequestSchema, aiResultSchema } from '@erpx/server-collaboration/domain/social-ai';
export type { AiResult } from '@erpx/server-collaboration/domain/social-ai';
