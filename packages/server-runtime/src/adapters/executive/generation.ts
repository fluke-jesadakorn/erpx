import { executiveGenerationQuota } from '@erpx/server-agent/public/executive-generation-quota';
import { clearExecutiveGenerationAssertion } from '@erpx/server-workflow/public/executive-generation-assertions';
import { executiveGenerationSourceLock } from '@erpx/server-platform_administration/public/executive-generation-source-lock';
import { guardExecutiveGenerationWorkspace } from '@erpx/server-organization/public/executive-generation-workspace-guard';
import { aiRequestQuotaLock } from "@erpx/server-agent/public/ai-quota";
import "server-only";
import { z } from "zod";
import { generateSchema, type Generation } from "@erpx/feature-shared/lib/executive/canvas";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { BetaEnvironment } from "@erpx/server-manage_erpx/public/beta-config";
import { ApiError } from "@erpx/server-runtime/public/http";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand} from "@erpx/server-workflow/public/commands";
import { aiSnapshotGuard, aiUsageStatement, resolveAiProvider } from "@erpx/server-agent/public/ai-providers";
import { generateProviderJson, withAiGatewayScope } from "@erpx/server-agent/public/ai-transport";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { requireAuthor } from "@erpx/server-runtime/public/executive/access";
import { reportAccess } from "@erpx/server-runtime/public/executive/reports";
import { executiveAiContext } from "@erpx/server-runtime/public/executive/context";
import { assertProductionModuleEnabled, productionFeatureGuard, scheduledFeaturePredicate } from "@erpx/server-pms/public/hospitality-rollout";
import type { SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import type { Job } from '@erpx/server-runtime/domain/executive/generation';
import { createExecutiveGenerationApplication, type ExecutiveGenerationRepository } from '@erpx/server-runtime/application/executive/generation';
import { BusinessRuleError } from '@erpx/server-shared/index';
const scheduledGeneration = (company: string) => `(${scheduledFeaturePredicate(company, "operations")} AND ${scheduledFeaturePredicate(company, "agent")})`;
const generationFeatureGuards = (db: SqlDatabase, key: string, company: string) => process.env.NODE_ENV !== "production" ? [] : [
    executiveGenerationSourceLock(db, company),
    ...productionFeatureGuard(db, key, company, "agent"),
    guardExecutiveGenerationWorkspace(db, key, company),
];
function application(database: SqlDatabase, env: BetaEnvironment, generate = generateProviderJson) { const repository: ExecutiveGenerationRepository<SqlStatement> = { ...executiveGenerationQuota(database), source2(id: string, userId: string, assignmentId: string, value: string) { return database.prepare("SELECT * FROM executive_generation_jobs WHERE report_id=? AND user_id=? AND assignment_id=? AND scope_key=? ORDER BY created_at DESC LIMIT 30").bind(id, userId, assignmentId, value).all<Job>(); }, source3(key: string) { return database.prepare("SELECT * FROM executive_generation_jobs WHERE id=?").bind(key).first<Job>(); }, guard5(key: string, value: "executive-rate", companyId: string, value2: number) { return assertion(database, key, value, "(SELECT count(*) FROM executive_generation_jobs WHERE company_id=? AND created_at>?)<10", [companyId, value2]); }, guard6(key: string, value: "executive-concurrency", companyId: string) { return assertion(database, key, value, "(SELECT count(*) FROM executive_generation_jobs WHERE company_id=? AND state IN ('queued','running'))<3", [companyId]); }, source7(key: string, id: string, companyId: string, userId: string, assignmentId: string, value: string, question: string, value2: string, value3: string, value4: string, now: number, now2: number) { return database.prepare("INSERT INTO executive_generation_jobs(id,report_id,company_id,user_id,assignment_id,scope_key,question,input_json,actor_json,provider_json,state,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,'queued',?,?)").bind(key, id, companyId, userId, assignmentId, value, question, value2, value3, value4, now, now2); }, source8(key: string) { return clearExecutiveGenerationAssertion(database, key); }, source9(now: number, jobId: string, id: string, userId: string, assignmentId: string) { return database.prepare("UPDATE executive_generation_jobs SET state='cancelled',updated_at=? WHERE id=? AND report_id=? AND user_id=? AND assignment_id=? AND state IN ('queued','running')").bind(now, jobId, id, userId, assignmentId); }, source10(now: number, now2: number) { return database.prepare(`UPDATE executive_generation_jobs j SET state='failed',error='Generation was interrupted. Your saved report is unchanged; retry to start a new attempt.',updated_at=? WHERE state='running' AND lease_until<? AND ${scheduledGeneration("j.company_id")}`).bind(now, now2).run(); }, source11(token: string, value: number, now: number) { return database.prepare(`UPDATE executive_generation_jobs SET state='running',lease_token=?,lease_until=?,attempts=attempts+1,updated_at=? WHERE id=(SELECT id FROM executive_generation_jobs queued WHERE state='queued' AND ${scheduledGeneration("queued.company_id")} ORDER BY created_at FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING *`).bind(token, value, now).first<Job>(); }, guard13(id: string, value: "generation-lease", id2: string, token: string, now: number) { return assertion(database, id, value, "EXISTS(SELECT 1 FROM executive_generation_jobs WHERE id=? AND state='running' AND lease_token=? AND lease_until>?)", [id2, token, now]); }, source13(value: string, now: number, id: string) { return database.prepare("UPDATE executive_generation_jobs SET state='ready',proposal_json=?,updated_at=? WHERE id=?").bind(value, now, id); }, source14(id: string) { return clearExecutiveGenerationAssertion(database, id); }, source15(value: string, now: number, id: string, token: string) { return database.prepare("UPDATE executive_generation_jobs SET state='failed',error=?,updated_at=? WHERE id=? AND state='running' AND lease_token=?").bind(value, now, id, token); }, source16(value: string) { return clearExecutiveGenerationAssertion(database, value); } }; return createExecutiveGenerationApplication<SqlStatement>({ repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, commands: { replay: (...args) => import('@erpx/server-workflow/public/commands').then(x => x.replayCommand(database, ...args)), commit: (...args) => executeCommand(database, ...args) }, batch: effects => database.batch(effects), sha256, schemaRejected: issues => console.warn('Executive proposal schema rejected', issues), generate, withAiGatewayScope, resolveAiProvider: (company, kind) => fromNative(() => resolveAiProvider(database, company, env, kind)), generationFeatureGuards: (key, company) => generationFeatureGuards(database, key, company), aiRequestQuotaLock: (...args) => fromNativeSync(() => aiRequestQuotaLock(database, ...args)), requireAuthor: (...args) => fromNative(() => requireAuthor(database, ...args)), reportAccess: (...args) => fromNative(() => reportAccess(database, ...args)), executiveAiContext: (...args) => fromNative(() => executiveAiContext(database, ...args)), assertProductionModuleEnabled: (...args) => fromNative(() => assertProductionModuleEnabled(database, ...args)), aiSnapshotGuard: (...args) => fromNativeSync(() => aiSnapshotGuard(database, ...args)), aiUsageStatement: (...args) => fromNativeSync(() => aiUsageStatement(database, ...args)) }); }
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
export async function drainExecutiveGeneration(db: SqlDatabase, env: BetaEnvironment, generate = generateProviderJson) { return invoke(() => application(db, env, generate).drainExecutiveGeneration()); }
