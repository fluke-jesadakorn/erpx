import {withWorkSocialAiPhase,withWorkSocialQueueClaim,withWorkSocialQueueMaintenance,withWorkSocialCommand} from '@erpx/server-collaboration/adapters/work-social-purchases';
import { socialReplyEngineeringLock } from '@erpx/server-platform_administration/public/collaboration-reply-rollout';
import { socialReplyCompanyGuard } from '@erpx/server-organization/public/collaboration-reply-rollout';
import 'server-only';
import type { AiEndpoint as NativeAiEndpoint } from '@erpx/server-agent/public/ai-transport';
import sharp from 'sharp';
import type { StaffActor } from '@erpx/server-operations/public/staff-policy';
import type { BetaEnvironment } from '@erpx/server-manage_erpx/public/beta-config';
import type { SqlDatabase, SqlStatement, FileStore } from '@erpx/server-runtime/public/runtime/types';
import { ApiError } from '@erpx/server-runtime/public/http';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { sha256 } from '@erpx/server-runtime/public/crypto';
import { socialAgentAssignmentGuard } from '@erpx/server-identity_access/public/collaboration-agent-authority';
import { captureJobAuthority, requireJobAuthority } from '@erpx/server-identity_access/public/iam-jobs';
import { iamMutationGuards } from '@erpx/server-identity_access/public/iam-store';
import {readablePost,sourceSnapshot,sourceGuard} from '@erpx/server-collaboration/public/social-source';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand} from '@erpx/server-workflow/public/commands';
import { resolveAiProvider, aiSnapshotGuard, aiUsageStatement } from '@erpx/server-agent/public/ai-providers';
import { generateProviderJson, withAiGatewayScope } from '@erpx/server-agent/public/ai-transport';
import { assertProductionModuleEnabled, productionFeatureGuard, scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
import { aiRequestQuotaLock } from '@erpx/server-agent/public/ai-quota';
import { collaborationAgentSources as filesFacts } from '@erpx/server-files/public/collaboration-agent-sources';
import { collaborationAgentSources as agent } from '@erpx/server-agent/public/collaboration-agent-sources';
import { collaborationAgentSources as workflow } from '@erpx/server-workflow/public/collaboration-agent-sources';
import type { Reply } from '@erpx/server-collaboration/domain/social-agent';
import { createSocialAgentApplication, type SocialAgentRepository } from '@erpx/server-collaboration/application/social-agent';
const scheduledReply = (company: string) => `(${scheduledFeaturePredicate(company, "collaboration")} AND ${scheduledFeaturePredicate(company, "agent")})`;
const replyFeatureGuards = (db: SqlDatabase, key: string, company: string) => process.env.NODE_ENV !== "production" ? [] : [
    socialReplyEngineeringLock(db, company),
    ...productionFeatureGuard(db, key, company, "agent"),
    socialReplyCompanyGuard(db, key, company),
];
async function inward<T>(action: () => Promise<T>) { try {
    return await action();
}
catch (error) {
    if (error instanceof ApiError)
        throw new BusinessRuleError(error.status, error.code, error.message);
    throw error;
} }
async function exposed<T>(action: () => Promise<T>) { try {
    return await action();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new ApiError(error.status, error.code, error.message);
    throw error;
} }
function application(db: SqlDatabase, files?: FileStore) { const owner=db;const repository: SocialAgentRepository<SqlStatement> = { async requestReply(id: string, companyId: string, userId: string) { return db.prepare("SELECT 1").bind().first<Reply>(); }, async triggerComment(owner_id: string, post_id: string) { return db.prepare("SELECT 1").bind().first(); }, guardFailedReply(key: string, value: "failed", id: string) { return assertion(db, key, value, "EXISTS(SELECT 1 FROM social_agent_replies WHERE id=? AND state='failed')", [id]); }, prepareRetry(value: string, now: number, id: string) { return db.prepare("SELECT 1").bind(); }, async expireLeases(now: number, now2: number) { return withWorkSocialQueueMaintenance(owner,now,transaction=>transaction.prepare(`UPDATE social_agent_replies r SET state='failed',error='The assistant request timed out. Try again.',error_code='AI_TIMEOUT',updated_at=? WHERE state='running' AND lease_until<? AND ${scheduledReply("r.company_id")}`).bind(now, now2).run()); }, async claimReply(value: string, value2: number, now: number, value3: string | null, value4: string | null) { return withWorkSocialQueueClaim(owner,{token:value,leaseUntil:value2,now,postId:value3},transaction=>transaction.prepare(`UPDATE social_agent_replies SET state='running',attempt=attempt+1,lease_token=?,lease_until=?,updated_at=? WHERE id=(SELECT id FROM social_agent_replies pending WHERE state='pending' AND (?::text IS NULL OR post_id=?) AND ${scheduledReply("pending.company_id")} ORDER BY created_at,id FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING *`).bind(value,value2,now,value3,value4).first<Reply>()); }, async recentComments(post_id: string, now: number) { return db.prepare("SELECT 1").bind().all<{
        id: string;
    }>(); }, async postContent(post_id: string, companyId: string) { return db.prepare("SELECT 1").bind().first<{
        content_json: string | null;
    }>(); }, async commentContent(owner_id: string, post_id: string) { return db.prepare("SELECT 1").bind().first<{
        rich_content_json: string | null;
    }>(); }, lockSocialQuota(value: string) { return db.prepare("SELECT 1").bind(); }, prepareReplyDraft(usageId: string, post_id: string, userId: string, version: number, hash: string, value: string, value2: string, value3: string, now: number, value4: number, value5: string | null) { return db.prepare("SELECT 1").bind(); }, reserveUsage(usageId: string, companyId: string, userId: string, value: string, value2: number, now: number) { return db.prepare("SELECT 1").bind(); }, async currentParent(owner_id: string, post_id: string) { return db.prepare("SELECT 1").bind().first<{
        id: string;
    }>(); }, guardCurrentLease(usageId: string, value: "lease", id: string, lease_token: string, finish: number) { return assertion(db, usageId, value, "EXISTS(SELECT 1 FROM social_agent_replies WHERE id=? AND state='running' AND lease_token=? AND lease_until>?)", [id, lease_token, finish]); }, prepareReplyComment(value: string, post_id: string, ASSISTANT_ID: "erpx-assistant", aiName: string, body: string, value2: string, usageId: string, finish: number, finish2: number, value3: string | null) { return db.prepare("SELECT 1").bind(); }, finishReply(finish: number, id: string) { return db.prepare("SELECT 1").bind(); }, finishDraft(value: string, usageId: string) { return db.prepare("SELECT 1").bind(); }, finishUsage(inputTokens: number, outputTokens: number, usageId: string) { return db.prepare("SELECT 1").bind(); }, failReply(explanation: string, code: string, now: number, id: string, lease_token: string,actorJson:string,ownerType:"post"|"comment",ownerId:string) { return db.prepare("SELECT 1").bind(); }, failDraft(usageId: string) { return db.prepare("SELECT 1").bind(); }, failUsage(usageId: string) { return db.prepare("SELECT 1").bind(); }, legacyPostImages:(...args)=>filesFacts(db).legacyPostImages(...args),imageSource:(...args)=>filesFacts(db).imageSource(...args),quotaAllowed:(...args)=>agent(db).quotaAllowed(...args),guardQuota:(...args)=>agent(db).guardQuota(...args),assistantName:(...args)=>agent(db).assistantName(...args),clearReserveAssertions:(...args)=>workflow(db).clearReserveAssertions(...args),clearFinishAssertions:(...args)=>workflow(db).clearFinishAssertions(...args),clearFailureAssertions:(...args)=>workflow(db).clearFailureAssertions(...args) }; return createSocialAgentApplication<SqlStatement>({ repository, clock: { now: () => Date.now() }, identifiers: { next: () => crypto.randomUUID() }, production: process.env.NODE_ENV === 'production', fileAvailable: Boolean(files), sha256, batch: effects => db.batch(effects),writePhase:(actor,phase,prepare)=>inward(()=>withWorkSocialAiPhase(owner,actor,phase,async transaction=>{const previous=db;db=transaction;try{return await transaction.batch(await prepare());}finally{db=previous;}})), readImagePhase:(actor,phase,read)=>inward(()=>withWorkSocialAiPhase(owner,actor,phase,async transaction=>{const previous=db;db=transaction;try{return await read();}finally{db=previous;}})),readObject: async (key) => { const object = await files?.get(key); return object ? object.arrayBuffer() : null; }, resizeImage: async (bytes) => 'data:image/jpeg;base64,' + (await sharp(Buffer.from(bytes)).resize(1200, 1200, { fit: 'inside', withoutEnlargement: true }).jpeg({ quality: 80 }).toBuffer()).toString('base64'), reportFailure: facts => console.warn(JSON.stringify(facts)), replyFeatureGuards: (...args) => replyFeatureGuards(db, ...args), assignmentGuard: (...args) => socialAgentAssignmentGuard(db, ...args), captureJobAuthority: (...args) => inward(() => captureJobAuthority(db, ...args)), requireJobAuthority, iamMutationGuards: (...args) => iamMutationGuards(db, ...args), readablePost: (...args) => inward(() => readablePost(db, ...args)), sourceSnapshot: (...args) => inward(() => sourceSnapshot(db, ...args)), sourceGuard: (...args) => sourceGuard(db, ...args), resolveAiProvider: (...args) => inward(() => resolveAiProvider(db, ...args)), aiSnapshotGuard: (...args) => aiSnapshotGuard(db, ...args), aiUsageStatement: (...args) => aiUsageStatement(db, ...args), generateProviderJson: (endpoint, messages, tokens) => inward(() => generateProviderJson(endpoint as NativeAiEndpoint, messages, tokens)), withAiGatewayScope: (endpoint, ...args) => withAiGatewayScope(endpoint as NativeAiEndpoint, ...args), assertProductionModuleEnabled: (...args) => inward(() => assertProductionModuleEnabled(db, ...args)), aiRequestQuotaLock: (...args) => aiRequestQuotaLock(db, ...args), commands: { commit: (...args) => inward(() => executeCommand(db, ...args)) } }); }
export function retryAgentReply(db: SqlDatabase, actor: StaffActor, id: string, key: string) { return exposed(async()=>{const row=await db.prepare('SELECT post_id FROM social_agent_replies WHERE id=? AND company_id=? AND actor_id=?').bind(id,actor.companyId,actor.userId).first<{post_id:string}>();if(!row)throw new ApiError(404,'REPLY_UNAVAILABLE','This assistant request is unavailable.');return withWorkSocialCommand(db,actor,{key,operation:'social.agent.retry',type:'post',id:row.post_id,purpose:'settle-existing'},transaction=>application(transaction).retryAgentReply(actor,id,key));}); }
export function drainSocialAgent(db: SqlDatabase, env: BetaEnvironment, postId?: string, provider = generateProviderJson, files?: FileStore, holdingId?: string) { return exposed(() => application(db, files).drainSocialAgent(env, postId, (endpoint, messages, tokens) => inward(() => provider(endpoint as NativeAiEndpoint, messages, tokens)), holdingId, provider === generateProviderJson)); }
