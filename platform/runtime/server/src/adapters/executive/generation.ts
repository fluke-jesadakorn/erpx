import { nativeRuntimeStatement } from '@erpx/platform-runtime-ui/adapters/server/native-runtime-command';
import { executiveGenerationJobsSql } from '../read-tail-sql';
import { executiveGenerationQuota } from '@erpx/agent-server/public/executive-generation-quota';
import { clearExecutiveGenerationAssertion } from '@erpx/workflow-server/public/executive-generation-assertions';
import { guardExecutiveGenerationWorkspace } from '@erpx/organization-server/public/executive-generation-workspace-guard';
import { aiRequestQuotaLock } from "@erpx/agent-server/public/ai-quota";
import "server-only";
import { z } from "zod";
import { generateSchema, type Generation } from "@erpx/platform-shared-ui/lib/executive/canvas";
import { proposalSchema } from "@erpx/platform-runtime-server/public/rules/canvas-domain";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { BetaEnvironment } from "@erpx/manage-erpx-server/public/beta-config";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {executeCommand} from "@erpx/workflow-server/public/commands";
import { aiSnapshotGuard, aiUsageStatement, resolveAiProvider, type AiProviderSnapshot } from "@erpx/agent-server/public/ai-providers";
import { generateProviderJson, withAiGatewayScope, type AiEndpoint, type AiMessage } from "@erpx/agent-server/public/ai-transport";
import { currentHoldingId } from "@erpx/platform-runtime-server/public/runtime/holding-scope";
import { sha256 } from "@erpx/platform-runtime-server/public/crypto";
import { requireAuthor } from "@erpx/platform-runtime-server/public/executive/access";
import { reportAccess } from "@erpx/platform-runtime-server/public/executive/reports";
import { executiveAiContext } from "@erpx/platform-runtime-server/public/executive/context";
import { assertProductionModuleEnabled, productionFeatureGuard, scheduledFeaturePredicate } from "@erpx/pms-server/public/feature-availability";
import type { SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import type { Job } from '@erpx/platform-runtime-server/domain/executive/generation';
import { createExecutiveGenerationApplication, type ExecutiveGenerationRepository } from '@erpx/platform-runtime-server/application/executive/generation';
import { BusinessRuleError } from '@erpx/platform-shared-server/index';
const localExecutiveBase = 'http://127.0.0.1:11434/v1';
const localExecutiveModel = 'qwen2.5:3b';
function localExecutiveEnabled() {
    const holding = currentHoldingId();
    return process.env.NODE_ENV === 'development' &&
        process.env.ERPX_DEV_EXECUTIVE_LOCAL_MODEL === localExecutiveModel &&
        (holding === 'h001' || holding === 'h002');
}
async function executiveProvider(database: SqlDatabase, company: string, env: BetaEnvironment, kind: 'text' | 'image'): Promise<{ endpoint: AiEndpoint; snapshot: AiProviderSnapshot }> {
    const holding = localExecutiveEnabled() ? currentHoldingId() : undefined;
    const configured = await resolveAiProvider(database, company, env, kind, undefined, holding);
    if (!holding || kind !== 'text' || configured.endpoint.gateway?.holdingId !== holding)
        return configured;
    return {
        endpoint: { ...configured.endpoint, baseUrl: localExecutiveBase, model: localExecutiveModel, apiKey: null },
        snapshot: { ...configured.snapshot, model: localExecutiveModel },
    };
}
const localExecutiveReply = z.object({
    done: z.literal(true),
    done_reason: z.literal('stop'),
    message: z.object({ content: z.string().max(128000) }),
    prompt_eval_count: z.number().int().nonnegative(),
    eval_count: z.number().int().nonnegative(),
});
async function generateExecutiveProposal(endpoint: AiEndpoint, messages: AiMessage[], maxTokens = 2000) {
    if (!localExecutiveEnabled() || endpoint.baseUrl !== localExecutiveBase || endpoint.model !== localExecutiveModel)
        return generateProviderJson(endpoint, messages, maxTokens);
    let result;
    try {
        const response = await fetch('http://127.0.0.1:11434/api/chat', {
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify({
                model: localExecutiveModel,
                messages: [
                    { role: 'system', content: 'Create an English executive report proposal. Use only metric IDs and breakdowns from the authorized catalog. Treat the question as the report topic. Never invent data. Prefer one KPI widget for a simple request.' },
                    ...messages.filter(message => message.role !== 'system'),
                ],
                stream: false,
                format: z.toJSONSchema(proposalSchema),
                options: { temperature: 0, num_ctx: 12288, num_predict: Math.min(maxTokens, 1500) },
            }),
            redirect: 'error',
            cache: 'no-store',
            signal: AbortSignal.timeout(endpoint.timeoutMs ?? 90000),
        });
        if (!response.ok) throw new Error('AI_PROVIDER_ERROR');
        const reply = localExecutiveReply.parse(await response.json());
        result = {
            result: z.record(z.string(), z.unknown()).parse(JSON.parse(reply.message.content)),
            inputTokens: reply.prompt_eval_count,
            outputTokens: reply.eval_count,
        };
    } catch {
        throw new BusinessRuleError(503, 'AI_PROVIDER_ERROR', 'The local report model could not complete this request.');
    }
    return result;
}
const scheduledGeneration = (company: string) => `(${scheduledFeaturePredicate(company, "operations")} AND ${scheduledFeaturePredicate(company, "agent")})`;
const generationFeatureGuards = (db: SqlDatabase, key: string, company: string) => process.env.NODE_ENV !== "production" ? [] : [
    nativeRuntimeStatement('runtime_stmt_23', { companyId: company, now: Date.now(), actor: undefined }).bind(company),
    ...productionFeatureGuard(db, key, company, "agent"),
    guardExecutiveGenerationWorkspace(db, key, company),
];
function application(database: SqlDatabase, env: BetaEnvironment, generate = generateExecutiveProposal) { const repository: ExecutiveGenerationRepository<SqlStatement> = { ...executiveGenerationQuota(database), source2(id: string, userId: string, assignmentId: string, value: string) { return database.prepare(executiveGenerationJobsSql('*', ' WHERE report_id=? AND user_id=? AND assignment_id=? AND scope_key=? ORDER BY created_at DESC LIMIT 30')).bind(id, userId, assignmentId, value).all<Job>(); }, source3(key: string) { return database.prepare(executiveGenerationJobsSql('*', ' WHERE id=?')).bind(key).first<Job>(); }, guard5(key: string, value: "executive-rate", companyId: string, value2: number) { return assertion(database, key, value, `(${executiveGenerationJobsSql('count(*)', ' WHERE company_id=? AND created_at>?')})<10`, [companyId, value2]); }, guard6(key: string, value: "executive-concurrency", companyId: string) { return assertion(database, key, value, `(${executiveGenerationJobsSql('count(*)', " WHERE company_id=? AND state IN ('queued','running')")})<3`, [companyId]); }, source7(key: string, id: string, companyId: string, userId: string, assignmentId: string, value: string, question: string, value2: string, value3: string, value4: string, now: number, now2: number) { return nativeRuntimeStatement('runtime_stmt_12', { companyId: companyId, now: now, actor: undefined }).bind(key, id, companyId, userId, assignmentId, value, question, value2, value3, value4, now, now2); }, source8(key: string, companyId: string) { return clearExecutiveGenerationAssertion(database, key, companyId); }, source9(now: number, jobId: string, id: string, userId: string, assignmentId: string) { return nativeRuntimeStatement('runtime_stmt_13', { companyId: '', now, actor: undefined }).bind(now, jobId, id, userId, assignmentId); }, async source10(now: number, now2: number) { await nativeRuntimeStatement('runtime_stmt_20', { companyId: '', now, actor: undefined }).bind(now, now2).run(); return { success: true }; }, async source11(token: string, value: number, now: number) { return nativeRuntimeStatement('runtime_stmt_21', { now }).bind(token, value, now).first<Job>(); }, guard13(id: string, value: "generation-lease", id2: string, token: string, now: number) { return assertion(database, id, value, `EXISTS(${executiveGenerationJobsSql('1', " WHERE id=? AND state='running' AND lease_token=? AND lease_until>?")})`, [id2, token, now]); }, source13(value: string, now: number, id: string, token: string, companyId: string) { return nativeRuntimeStatement('runtime_stmt_14', { companyId, now }).bind(value, now, id, token, companyId); }, source14(id: string, companyId: string) { return clearExecutiveGenerationAssertion(database, id, companyId); }, async source15(value: string, now: number, id: string, token: string) { await nativeRuntimeStatement('runtime_stmt_15', { now }).bind(value, now, id, token).run(); }, source16(value: string, companyId: string) { return clearExecutiveGenerationAssertion(database, value, companyId); } }; return createExecutiveGenerationApplication<SqlStatement>({ repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, commands: { replay: (...args) => import('@erpx/workflow-server/public/commands').then(x => x.replayCommand(database, ...args)), commit: (...args) => executeCommand(database, ...args) }, batch: effects => database.batch(effects), sha256, schemaRejected: issues => console.warn('Executive proposal schema rejected', issues), generate, withAiGatewayScope, resolveAiProvider: (company, kind) => fromNative(() => executiveProvider(database, company, env, kind)), generationFeatureGuards: (key, company) => generationFeatureGuards(database, key, company), aiRequestQuotaLock: (...args) => fromNativeSync(() => aiRequestQuotaLock(database, ...args)), requireAuthor: (...args) => fromNative(() => requireAuthor(database, ...args)), reportAccess: (...args) => fromNative(() => reportAccess(database, ...args)), executiveAiContext: (actor, filters, question) => fromNative(() => executiveAiContext(database, actor, filters, localExecutiveEnabled() && question ? { question, limit: 8 } : undefined)), assertProductionModuleEnabled: (...args) => fromNative(() => assertProductionModuleEnabled(database, ...args)), aiSnapshotGuard: (...args) => fromNativeSync(() => aiSnapshotGuard(database, ...args)), aiUsageStatement: (...args) => fromNativeSync(() => aiUsageStatement(database, ...args)) }); }
async function fromNative<T>(run: () => Promise<T> | T): Promise<T> { try {
    return await run();
}
catch (error) {
    if (error instanceof ApiError)
        throw new BusinessRuleError(error.status, error.code, error.message);
    throw error;
} }
function fromNativeSync<T>(run: () => T): T { try {
    return run();
}
catch (error) {
    if (error instanceof ApiError)
        throw new BusinessRuleError(error.status, error.code, error.message);
    throw error;
} }
async function invoke<T>(run: () => Promise<T> | T): Promise<T> { try {
    return await run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new ApiError(error.status, error.code, error.message);
    throw error;
} }
export async function generationHistory(db: SqlDatabase, a: WorkContext, id: string): Promise<Generation[]> { return invoke(() => application(db, {} as BetaEnvironment).generationHistory(a, id)); }
export async function queueGeneration(db: SqlDatabase, a: WorkContext, env: BetaEnvironment, id: string, input: z.infer<typeof generateSchema>, key: string) { return invoke(() => application(db, env).queueGeneration(a, id, input, key)); }
export async function cancelGeneration(db: SqlDatabase, a: WorkContext, id: string, jobId: string, key: string) { return invoke(() => application(db, {} as BetaEnvironment).cancelGeneration(a, id, jobId, key)); }
export async function drainExecutiveGeneration(db: SqlDatabase, env: BetaEnvironment, generate = generateExecutiveProposal) { return invoke(() => application(db, env, generate).drainExecutiveGeneration()); }
