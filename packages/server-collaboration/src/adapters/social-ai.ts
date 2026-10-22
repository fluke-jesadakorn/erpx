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
import { sourceSnapshot, sourceGuard, readablePost } from '@erpx/server-collaboration/public/social-source';
import { staffPeople } from '@erpx/server-operations/public/staff-policy';
import { workContextGuards } from '@erpx/server-workflow/public/commands';
import { aiRequestQuotaLock } from '@erpx/server-agent/public/ai-quota';
import { resolveAiProvider, aiSnapshotGuard, aiUsageStatement } from '@erpx/server-agent/public/ai-providers';
import { generateProviderJson, withAiGatewayScope } from '@erpx/server-agent/public/ai-transport';
import { collaborationAiSources as agent } from '@erpx/server-agent/public/collaboration-ai-sources';
import { collaborationAiSources as workflow } from '@erpx/server-workflow/public/collaboration-ai-sources';
import { aiRequestSchema } from '@erpx/server-collaboration/domain/social-ai';
import { createSocialAiApplication, type SocialAiRepository } from '@erpx/server-collaboration/application/social-ai';
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
function application(db?: SqlDatabase) {
    const owner=db;
    const repository: SocialAiRepository<SqlStatement> = { async priorDraft(id: string, userId: string) {
            return db!
                .prepare("SELECT actor_json,post_id,kind,source_hash,result_json,state,source_version,comment_ids_json,expires_at,provider_json FROM social_ai_drafts WHERE id=? AND actor_id=?")
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
            return db!
                .prepare("INSERT INTO social_ai_drafts(id,post_id,actor_id,kind,source_version,source_hash,comment_ids_json,sources_json,state,model,created_at,expires_at) VALUES(?,?,?,?,?,?,?,?,'pending',?,?,?)")
                .bind(id, postId, userId, kind, version, hash, value, value2, value3, now, value4);
        }, reserveUsage(id: string, companyId: string, userId: string, month: string, value: number, now: number) {
            return db!
                .prepare("INSERT INTO social_ai_usage(id,company_id,actor_id,month,state,lease_until,created_at) VALUES(?,?,?,?,'reserved',?,?)")
                .bind(id, companyId, userId, month, value, now);
        }, captureProvider(value: string, id: string) {
            return db!
                .prepare("UPDATE social_ai_drafts SET provider_json=? WHERE id=?")
                .bind(value, id);
        }, captureActor(value: string, id: string) { return db!.prepare("UPDATE social_ai_drafts SET actor_json=? WHERE id=?").bind(value, id); }, finishDraft(value: string, id: string) {
            return db!
                .prepare("UPDATE social_ai_drafts SET result_json=?,state='ready' WHERE id=?")
                .bind(value, id);
        }, finishUsage(inputTokens: number, outputTokens: number, id: string) {
            return db!
                .prepare("UPDATE social_ai_usage SET state='succeeded',input_tokens=?,output_tokens=?,lease_until=0 WHERE id=?")
                .bind(inputTokens, outputTokens, id);
        }, failDraft(id: string) {
            return db!
                .prepare("UPDATE social_ai_drafts SET state='failed',result_json=NULL WHERE id=?")
                .bind(id);
        }, failUsage(id: string) {
            return db!
                .prepare("UPDATE social_ai_usage SET state='failed',lease_until=0 WHERE id=?")
                .bind(id);
        }, guardQuota:(...args)=>agent(db!).guardQuota(...args),clearReserveAssertions:(...args)=>workflow(db!).clearReserveAssertions(...args),clearFinishAssertions:(...args)=>workflow(db!).clearFinishAssertions(...args) };
    return createSocialAiApplication<SqlStatement>({ repository, clock: { now: () => Date.now() }, identifiers: { next: () => crypto.randomUUID() }, sha256, batch: effects => db!.batch(effects), writePhase:(actor,phase,prepare)=>inward(()=>withWorkSocialAiPhase(owner!,actor,phase,async transaction=>{const previous=db;db=transaction;try{return await transaction.batch(await prepare());}finally{db=previous;}})), sourceSnapshot: (...args) => inward(() => sourceSnapshot(db!, ...args)), sourceGuard: (...args) => sourceGuard(db!, ...args), readablePost: (...args) => inward(() => readablePost(db!, ...args)), staffPeople: (...args) => inward(() => staffPeople(db!, ...args)), workContextGuards: (...args) => workContextGuards(db!, ...args), aiRequestQuotaLock: (...args) => aiRequestQuotaLock(db!, ...args), resolveAiProvider: (...args) => inward(() => resolveAiProvider(db!, ...args)), aiSnapshotGuard: (...args) => aiSnapshotGuard(db!, ...args), aiUsageStatement: (...args) => aiUsageStatement(db!, ...args), generateProviderJson: (endpoint, messages, tokens) => inward(() => generateProviderJson(endpoint as NativeAiEndpoint, messages, tokens)), withAiGatewayScope: (endpoint, ...args) => withAiGatewayScope(endpoint as NativeAiEndpoint, ...args) });
}
export function callAi(env: BetaEnvironment, input: unknown) { return exposed(() => application().callAi(env, input)); }
export function createAiDraft(db: SqlDatabase, actor: StaffActor, postId: string, input: z.infer<typeof aiRequestSchema>, env: BetaEnvironment, provider = callAi, commandKey?: string) { return exposed(() => application(db).createAiDraft(actor, postId, input, env, provider, commandKey, provider === callAi)); }
export { aiRequestSchema, aiResultSchema } from '@erpx/server-collaboration/domain/social-ai';
export type { AiResult } from '@erpx/server-collaboration/domain/social-ai';
