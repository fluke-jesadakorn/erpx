import {peoplePackageEnforcementMode,readPurchasedAccess} from "@erpx/server-commercial/public/purchase-command-boundary";
import {captureExpenseQueueActor,withExpensePurchasedRead,withExpensePurchasedCommand,expenseFinalRead} from "@erpx/server-people_payroll/adapters/expense-purchases";
import {withExpenseAiPurchasedClaim,withExpenseAiPurchasedMutation,expenseJobUnavailable} from "@erpx/server-people_payroll/adapters/expense-ai-purchases";
import { hospitalityOwnerProjectionSources as agentProjectionSources } from "@erpx/server-agent/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as collaborationProjectionSources } from "@erpx/server-collaboration/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as commerceProjectionSources } from "@erpx/server-commerce/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as manage_erpxProjectionSources } from "@erpx/server-manage_erpx/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as recordsProjectionSources } from "@erpx/server-records/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as runtimeProjectionSources } from "@erpx/server-runtime/public/hospitality-owner-projection-sources";
import { expenseNativeEffects as workflowExpenseEffects } from "@erpx/server-workflow/public/expense-native-effects";
import { createQueueExpenseAi, createRunExpenseAi, createDrainExpenseAi, type ExpenseAiJob, type ExpenseAiUsage } from "@erpx/server-people_payroll/application/expense-ai-jobs";
import { expenseAiInstruction } from "@erpx/server-people_payroll/domain/expense-ai-processing";
import { expenseCommandPort, expenseMutation } from "@erpx/server-people_payroll/adapters/expense-command-port";
import { createExpenseAiStatus } from "@erpx/server-people_payroll/application/expense-ai-status";
/** Native Expense adapter; preserves legacy persistence and command semantics. */
import {aiRequestQuotaLock} from "@erpx/server-agent/public/ai-quota";
import {requireJobAuthority} from "@erpx/server-identity_access/public/iam-jobs";
import { sha256 as aiSessionHash } from "@erpx/server-runtime/public/crypto";
import type { SqlDatabase, FileStore } from "@erpx/server-runtime/public/runtime/types";
import { ApiError } from "@erpx/server-runtime/public/http";
import { assertProductionModuleEnabled, productionFeatureGuard } from "@erpx/server-pms/public/feature-availability";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import type { StaffActor } from "@erpx/server-operations/public/staff-policy";
import type { BetaEnvironment } from "@erpx/server-manage_erpx/public/beta-config";
import { expenseAuthoritySnapshot } from "@erpx/server-people_payroll/adapters/expense-routing";
import {
  expenseAiSource,
  expenseAiAllowed,
  type AiClaim,
} from "@erpx/server-people_payroll/public/expense-ai-source";
import {
  resolveAiProvider,
  aiSnapshotGuard,
  aiUsageStatement,
  type AiProviderSnapshot,
} from "@erpx/server-agent/public/ai-providers";
import { generateProviderJson, withAiGatewayScope, type AiMessage } from "@erpx/server-agent/public/ai-transport";
type Providers = { text: AiProviderSnapshot; image: AiProviderSnapshot };
type Proposal = {
  id: string;
  company_id: string;
  expense_id: string;
  payment_draft_id: string | null;
  stage: string;
  source_version: number;
  evidence_digest: string;
  provider_json: string;
  state: string;
  result_json: string | null;
  error: string | null;
  requested_by: string;
  actor_json: string | null;
  finance_policy_version: number | null;
  attempts: number;
  claim_until: number | null;
  lease_token: string | null;
};
async function providers(
  db: SqlDatabase,
  companyId: string,
  env: BetaEnvironment,
) {
  const [text, image] = await Promise.all([
    resolveAiProvider(db, companyId, env, "text"),
    resolveAiProvider(db, companyId, env, "image"),
  ]);
  return {
    text,
    image,
    snapshot: {
      text: text.snapshot,
      image: image.snapshot,
    } satisfies Providers,
  };
}
function providerGuards(db: SqlDatabase, key: string, snapshot: Providers) {
  return [
    aiSnapshotGuard(db, key, snapshot.text, "ai-text-revision"),
    aiSnapshotGuard(db, key, snapshot.image, "ai-image-revision"),
  ];
}

async function receiptPolicy(db: SqlDatabase, companyId: string, version?: number | null) {
  if (version == null) return null;
  const policy = await db.prepare(`SELECT version,timezone,daily_limit,monthly_limit FROM (${financeProjectionSources.finance_ai_policies}) finance_ai_policies WHERE company_id=? AND enabled AND version=? AND jsonb_exists(kinds_json::jsonb,'receipt_extraction')`).bind(companyId,version).first<{version:number;timezone:string;daily_limit:number;monthly_limit:number}>();
  if (!policy) throw new ApiError(409,"AI_STALE","The Company Finance AI policy changed. Prepare receipt suggestions manually or review the policy.");
  await assertProductionModuleEnabled(db,companyId,"finance");
  return policy;
}
function receiptPolicyGuards(db: SqlDatabase, companyId: string, version: number | null | undefined, key: string) {
  return version == null ? [] : [...productionFeatureGuard(db,key,companyId,"finance"),assertion(db,key,"receipt-finance-policy",`EXISTS(SELECT 1 FROM (${financeProjectionSources.finance_ai_policies}) finance_ai_policies WHERE company_id=? AND enabled AND version=? AND jsonb_exists(kinds_json::jsonb,'receipt_extraction'))`,[companyId,version])];
}

/** Provider resolution and persistence remain bound to the caller's native database. */
export function nativeExpenseAiStatus(db: SqlDatabase, env: BetaEnvironment) {
  return createExpenseAiStatus<StaffActor, AiClaim>({
    source: (actor, id, paymentId) => expenseAiSource(db, actor.companyId, id, paymentId),
    allowed: (actor, claim, paymentId) => expenseAiAllowed(db, actor, claim, paymentId),
    async proposals(actor, id, paymentId) {
      return (await db.prepare("SELECT * FROM expense_ai_proposals WHERE expense_id=? AND company_id=? AND payment_draft_id IS NOT DISTINCT FROM ? ORDER BY created_at DESC LIMIT 5")
        .bind(id, actor.companyId, paymentId).all<Proposal>()).results;
    },
    async providerConfiguration(actor) {
      try {
        const configuration = await providers(db, actor.companyId, env);
        return { snapshot: JSON.stringify(configuration.snapshot), unavailable: null };
      } catch (error) {
        return { snapshot: null, unavailable: error instanceof ApiError ? error.message : "AI is unavailable. Continue manually." };
      }
    },
    async financePolicyVersion(actor) {
      const policy = await db.prepare(`SELECT version FROM (${financeProjectionSources.finance_ai_policies}) finance_ai_policies WHERE company_id=? AND enabled AND jsonb_exists(kinds_json::jsonb,'receipt_extraction')`)
        .bind(actor.companyId).first<{ version: number }>();
      return policy?.version;
    },
  });
}
export async function expenseAiStatus(db: SqlDatabase, actor: StaffActor, env: BetaEnvironment, id: string, paymentId: string | null) {
  return withExpensePurchasedRead(db,actor,async database=>{const result=await nativeExpenseAiStatus(database,env)(actor,id,paymentId),mode=await peoplePackageEnforcementMode();if(mode==='disabled')return result;const access=await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:'people.expenses'});return {...result,allowed:result.allowed&&access.newWork};});
}
/** One native database/transaction owns the proposal, quota and command receipt. */
function originalNativeExpenseAiQueue(db: SqlDatabase, env: BetaEnvironment) {
  return createQueueExpenseAi({
    ...expenseCommandPort<StaffActor>(db),
    captureActor: actor => captureExpenseQueueActor(db, actor),
    financePolicy: (companyId, version) => receiptPolicy(db, companyId, version),
    async enabled(companyId) { await assertProductionModuleEnabled(db, companyId, "people_payroll"); },
    authority: companyId => expenseAuthoritySnapshot(db, companyId),
    source: async (companyId, id, paymentId) => { const native = await expenseAiSource(db, companyId, id, paymentId); return { ...native, native }; },
    allowed: (actor, claim, paymentId) => expenseAiAllowed(db, actor, claim, paymentId),
    configuration: async companyId => { const native = await providers(db, companyId, env); return { snapshot: JSON.stringify(native.snapshot), native }; },
    existing: (actor, id, input, source, configuration) => db.prepare("SELECT id,state FROM expense_ai_proposals WHERE company_id=? AND expense_id=? AND payment_draft_id IS NOT DISTINCT FROM ? AND stage=? AND source_version=? AND evidence_digest=? AND provider_json=? AND actor_json=? ORDER BY created_at DESC LIMIT 1")
      .bind(actor.companyId, id, input.paymentId, source.claim.workflow_stage, source.sourceVersion, source.digest, configuration.snapshot, JSON.stringify(actor)).first<{ id: string; state: string }>(),
    now: Date.now,
    month: now => new Date(now).toISOString().slice(0, 7),
    async quota(actor, month, now) { return await db
    .prepare(
      `SELECT (SELECT COUNT(*) FROM (${collaborationProjectionSources.social_ai_usage}) social_ai_usage WHERE company_id=? AND month=?)+(SELECT COUNT(*) FROM (SELECT company_id,month FROM expense_ai_usage UNION ALL SELECT company_id,month FROM (${recordsProjectionSources.request_ai_usage}) request_ai_usage UNION ALL SELECT company_id,month FROM (${financeProjectionSources.finance_ai_usage}) finance_ai_usage UNION ALL SELECT company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM') AS month FROM (${agentProjectionSources.ai_workspace_turns}) ai_workspace_turns UNION ALL SELECT company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM') AS month FROM (${runtimeProjectionSources.executive_generation_jobs}) executive_generation_jobs) WHERE company_id=? AND month=?) used,COALESCE((SELECT ai_monthly_limit FROM (${manage_erpxProjectionSources.beta_policy}) beta_policy WHERE company_id=?),100) allowance,(SELECT COUNT(*) FROM expense_ai_usage WHERE company_id=? AND state='reserved' AND lease_until>?) busy`,
    )
    .bind(
      actor.companyId,
      month,
      actor.companyId,
      month,
      actor.companyId,
      actor.companyId,
      now,
    )
    .first<{ used: number; allowance: number; busy: number }>(); },
    async prepareQueue(actor, id, input, key, capturedSource, capturedConfiguration, financePolicy, capturedAuthority, existing, now, month) {
      const source = capturedSource.native as Awaited<ReturnType<typeof expenseAiSource>>;
      const configuration = capturedConfiguration.native as Awaited<ReturnType<typeof providers>>;
      const authority = capturedAuthority as Awaited<ReturnType<typeof expenseAuthoritySnapshot>>;
      const proposalId = `expense-ai-${key}`, snapshot = JSON.stringify(configuration.snapshot);
      return expenseMutation({ id, proposalId, state: "queued" }, [
      ...(financePolicy ? [db.prepare(`SELECT company_id FROM (${financeProjectionSources.finance_ai_policies}) finance_ai_policies WHERE company_id=? FOR UPDATE`).bind(actor.companyId), assertion(db,key,"receipt-finance-budget",`(SELECT count(*) FROM (SELECT company_id,business_date FROM (${financeProjectionSources.finance_ai_usage}) finance_ai_usage UNION ALL SELECT company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE ?,'YYYY-MM-DD') FROM expense_ai_proposals WHERE finance_policy_version IS NOT NULL) calls WHERE company_id=? AND business_date=?)<? AND (SELECT count(*) FROM (SELECT company_id,month FROM (${financeProjectionSources.finance_ai_usage}) finance_ai_usage UNION ALL SELECT company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM') FROM expense_ai_proposals WHERE finance_policy_version IS NOT NULL) calls WHERE company_id=? AND month=?)<?`,[financePolicy.timezone,actor.companyId,new Intl.DateTimeFormat("en-CA",{timeZone:financePolicy.timezone,year:"numeric",month:"2-digit",day:"2-digit"}).format(new Date(now)),financePolicy.daily_limit,actor.companyId,month,financePolicy.monthly_limit])] : []),
      ...receiptPolicyGuards(db,actor.companyId,input.financePolicyVersion,key),
      authority.guard(key),
      ...source.guards(key),
      ...providerGuards(db, key, configuration.snapshot),
      aiRequestQuotaLock(db,actor.companyId),
      assertion(
        db,
        key,
        "ai-quota",
        `(SELECT COUNT(*) FROM (${collaborationProjectionSources.social_ai_usage}) social_ai_usage WHERE company_id=? AND month=?)+(SELECT COUNT(*) FROM (SELECT company_id,month FROM expense_ai_usage UNION ALL SELECT company_id,month FROM (${recordsProjectionSources.request_ai_usage}) request_ai_usage UNION ALL SELECT company_id,month FROM (${financeProjectionSources.finance_ai_usage}) finance_ai_usage UNION ALL SELECT company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM') AS month FROM (${agentProjectionSources.ai_workspace_turns}) ai_workspace_turns UNION ALL SELECT company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM') AS month FROM (${runtimeProjectionSources.executive_generation_jobs}) executive_generation_jobs) WHERE company_id=? AND month=?)<COALESCE((SELECT ai_monthly_limit FROM (${manage_erpxProjectionSources.beta_policy}) beta_policy WHERE company_id=?),100) AND (SELECT COUNT(*) FROM expense_ai_usage WHERE company_id=? AND state='reserved' AND lease_until>?)<2 AND (SELECT COUNT(*) FROM expense_ai_usage WHERE actor_id=? AND created_at>?)<6`,
        [
          actor.companyId,
          month,
          actor.companyId,
          month,
          actor.companyId,
          actor.companyId,
          now,
          actor.userId,
          now - 60000,
        ],
      ),
      ...(existing?.state === "ready" && input.retry ? [db.prepare(
        "UPDATE expense_ai_proposals SET state='stale',updated_at=? WHERE id=? AND company_id=? AND state='ready'",
      ).bind(now, existing.id, actor.companyId)] : []),
      db
        .prepare(
          "INSERT INTO expense_ai_proposals(id,company_id,expense_id,payment_draft_id,stage,source_version,evidence_digest,provider_json,state,requested_by,created_at,updated_at,actor_json,finance_policy_version) VALUES(?,?,?,?,?,?,?,?,'queued',?,?,?,?,?)",
        )
        .bind(
          proposalId,
          actor.companyId,
          id,
          input.paymentId,
          source.claim.workflow_stage,
          source.sourceVersion,
          source.digest,
          snapshot,
          actor.userId,
          now,
          now,
          JSON.stringify(actor),
          input.financePolicyVersion ?? null,
        ),
      db
        .prepare(
          "INSERT INTO expense_ai_usage(id,company_id,actor_id,month,state,lease_until,created_at) VALUES(?,?,?,?,'reserved',?,?)",
        )
        .bind(
          proposalId,
          actor.companyId,
          actor.userId,
          month,
          now + 15 * 60000,
          now,
        ),
    ]);
    },
  });
}
export async function queueExpenseAi(db: SqlDatabase, actor: StaffActor, env: BetaEnvironment, id: string, input: { paymentId: string | null; expectedVersion: number; retry: boolean; financePolicyVersion?: number }, key: string) {
  return withExpensePurchasedCommand(db,actor,{key,purpose:"new-work"},tx => nativeExpenseAiQueue(tx, env)(actor, id, input, key));
}
const encode = (buffer: ArrayBuffer) => {
  let text = "";
  for (const byte of new Uint8Array(buffer)) text += String.fromCharCode(byte);
  return btoa(text);
};
/** A queue job reads at most two image pages per provider call and never performs a business transition. */
function expenseAiUsageStatements(db: SqlDatabase, id: string, lease: string, row: ExpenseAiJob, actor: StaffActor | null, usage: ExpenseAiUsage, status: "succeeded" | "failed") {
  const snapshot = JSON.parse(row.provider_json) as Providers;
  return (["image", "text"] as const).filter(kind => usage[kind].attempted).map(kind => aiUsageStatement(db, `${id}:${lease}:${kind}`, snapshot[kind], "expense", status, usage[kind].input, usage[kind].output, actor?.branchId ?? null));
}
/** Timers, file bytes, provider protocols and atomic lease-guarded batches are native effects. */
export function nativeExpenseAiWorker(db: SqlDatabase, files: FileStore, env: BetaEnvironment, generate = generateProviderJson) {
  return createRunExpenseAi({
    error: violation => new ApiError(violation.status, violation.code, violation.message),
    async claim(id) {
      try { return await withExpenseAiPurchasedClaim(db,id,async database=>{
      const db=database;
      const now = Date.now(), lease = crypto.randomUUID();
      const claimed = await db.prepare("UPDATE expense_ai_proposals SET state='running',lease_token=?,claim_until=?,attempts=attempts+1,updated_at=? WHERE id=? AND (state='queued' OR (state='running' AND claim_until<?))")
        .bind(lease, now + 120000, now, id, now).run();
      if (claimed.meta.changes !== 1) return null;
      const row = (await db.prepare("SELECT * FROM expense_ai_proposals WHERE id=?").bind(id).first<Proposal>())!;
      return { row, lease };
      }); } catch(error) { if(expenseJobUnavailable(error))return null;throw error; }
    },
    renewal(id, lease) {
      let renewing: Promise<void> | null = null;
      const renewal = setInterval(() => {
        if (renewing) return;
        renewing = db.prepare("UPDATE expense_ai_proposals SET claim_until=? WHERE id=? AND lease_token=? AND state='running'")
          .bind(Date.now() + 120000, id, lease).run().then(() => {}).catch(() => {}).finally(() => { renewing = null; });
      }, 30000);
      return async () => { clearInterval(renewal); await renewing; };
    },
    async enabled(companyId) { await assertProductionModuleEnabled(db, companyId, "people_payroll"); },
    authorize: requireJobAuthority,
    financePolicy: (companyId, version) => receiptPolicy(db, companyId, version),
    authority: companyId => expenseAuthoritySnapshot(db, companyId),
    source: async (companyId, id, paymentId) => { const native = await expenseAiSource(db, companyId, id, paymentId); return { ...native, native }; },
    configuration: async companyId => { const native = await providers(db, companyId, env); return { snapshot: JSON.stringify(native.snapshot), native }; },
    allowed: (actor, claim, paymentId) => expenseAiAllowed(db, actor, claim, paymentId,"settle-existing"),
    async accounts(companyId) { return (await db.prepare(`SELECT a.id,a.name,a.code,a.currency FROM (${financeProjectionSources.finance_accounts}) a JOIN (${commerceProjectionSources.commerce_settings}) s ON s.chart_id=a.chart_id AND s.company_id=a.company_id WHERE a.company_id=? AND a.category='expense' AND a.status='active'`).bind(companyId).all<{ id: string; name: string; currency: string }>()).results; },
    async periods(companyId) { return (await db.prepare(`SELECT id,name,start_date,end_date FROM (${financeProjectionSources.finance_periods}) finance_periods WHERE company_id=? AND state='open'`).bind(companyId).all<{ id: string }>()).results; },
    async renew(id, lease) { await withExpenseAiPurchasedMutation(db,id,async database=>{await database.prepare("UPDATE expense_ai_proposals SET claim_until=? WHERE id=? AND lease_token=? AND state='running'").bind(Date.now()+120000,id,lease).run();}); },
    async imageContent(context, batch) {
      const content: Exclude<AiMessage["content"], string> = [
          {
            type: "text",
            text: JSON.stringify({
              context,
              pages: batch.map((p) => ({
                attachmentId: p.attachmentId,
                page: p.page,
                name: p.name,
              })),
            }),
          },
        ];
      for (const page of batch) {
        const file = await files.get(page.key);
        if (!file || file.size > 2 * 1024 * 1024)
          throw new ApiError(
            503,
            "AI_DOCUMENT_UNAVAILABLE",
            "An evidence page is unavailable. Continue manually or retry.",
          );
        content.push({
          type: "image_url",
          image_url: {
            url: `data:${page.mediaType};base64,${encode(await file.arrayBuffer())}`,
          },
        });
      }
      return content;
    },
    async extract(capturedConfiguration, row, actor, lease, offset, capturedContent) {
      const configuration = capturedConfiguration.native as Awaited<ReturnType<typeof providers>>;
      const content = capturedContent as Exclude<AiMessage["content"], string>, instruction = expenseAiInstruction, id = row.id;
      return generate(
        { ...withAiGatewayScope(configuration.image.endpoint,`${id}:${lease}:image:${offset}`,"expense",actor?.branchId??null,`${id}:${lease}:image`), sessionId: await aiSessionHash(`expense:${row.company_id}:${id}`) },
        [
          { role: "system", content: instruction },
          { role: "user", content },
        ],
        6000,
      );
    },
    async consolidate(capturedConfiguration, row, actor, lease, context, partial) {
      const configuration = capturedConfiguration.native as Awaited<ReturnType<typeof providers>>;
      const instruction = expenseAiInstruction, id = row.id;
      return generate(
        { ...withAiGatewayScope(configuration.text.endpoint,`${id}:${lease}:text`,"expense",actor?.branchId??null), sessionId: await aiSessionHash(`expense:${row.company_id}:${id}`) },
        [
          {
            role: "system",
            content:
              instruction +
              " Consolidate the extracted batches. Conflicting field values must remain null and be listed in conflicts. Never pick an unsupported winner.",
          },
          {
            role: "user",
            content: JSON.stringify({ context, extractions: partial }),
          },
        ],
        4000,
      );
    },
    async complete(id, lease, row, actor, capturedAuthority, capturedSource, result, inputTokens, outputTokens, usage) {
      const apply=async(database:SqlDatabase,authority:Awaited<ReturnType<typeof expenseAuthoritySnapshot>>,source:Awaited<ReturnType<typeof expenseAiSource>>)=>{
      const db=database;
      const snapshot = JSON.parse(row.provider_json) as Providers;
      const usageStatements = (status: "succeeded" | "failed") => expenseAiUsageStatements(db, id, lease, row, actor, usage, status);
      await db.batch([
      ...receiptPolicyGuards(db,row.company_id,row.finance_policy_version,lease),
      ...productionFeatureGuard(db,lease,row.company_id,"people_payroll"),
      assertion(
        db,
        lease,
        "job-lease",
        "EXISTS(SELECT 1 FROM expense_ai_proposals WHERE id=? AND state='running' AND lease_token=?)",
        [id, lease],
      ),
      authority.guard(lease),
      ...source.guards(lease),
      ...providerGuards(db, lease, snapshot),
      db
        .prepare(
          "UPDATE expense_ai_proposals SET state='ready',result_json=?,error=NULL,claim_until=NULL,lease_token=NULL,updated_at=? WHERE id=?",
        )
        .bind(JSON.stringify(result), Date.now(), id),
      db
        .prepare(
          "UPDATE expense_ai_usage SET state='succeeded',lease_until=0,input_tokens=?,output_tokens=? WHERE id=?",
        )
        .bind(inputTokens, outputTokens, id),
      ...usageStatements("succeeded"),
      workflowExpenseEffects(db).clearAssertions({commandId:lease}),
    ]);
      };
      if(await peoplePackageEnforcementMode()==='disabled')return apply(db,capturedAuthority as Awaited<ReturnType<typeof expenseAuthoritySnapshot>>,capturedSource.native as Awaited<ReturnType<typeof expenseAiSource>>);
      await withExpenseAiPurchasedMutation(db,id,async database=>{
       const authority=await expenseAuthoritySnapshot(database,row.company_id),source=await expenseAiSource(database,row.company_id,row.expense_id,row.payment_draft_id),originalAuthority=capturedAuthority as Awaited<ReturnType<typeof expenseAuthoritySnapshot>>;
       if(JSON.stringify(authority.args)!==JSON.stringify(originalAuthority.args)||source.digest!==capturedSource.digest||!await expenseAiAllowed(database,actor,source.claim,row.payment_draft_id,'settle-existing'))throw new ApiError(409,'AI_STALE','The original Expense source or queued authority changed before completion.');
       await apply(database,authority,source);
       expenseFinalRead(database,actor,async()=>{
        if((await expenseAiSource(database,row.company_id,row.expense_id,row.payment_draft_id)).digest!==source.digest||JSON.stringify((await providers(database,row.company_id,env)).snapshot)!==row.provider_json||!await database.prepare(`SELECT ${authority.query} allowed`).bind(...authority.args).first<boolean>('allowed'))throw new ApiError(409,'AI_STALE','The original Expense source changed during completion.');
       });
      });
    },
    errorFacts: error => ({ code: error instanceof ApiError ? error.code : null, message: error instanceof ApiError ? error.message : null, constraint: /constraint/i.test(String(error)) }),
    async holdsLease(id, lease) { return Boolean(await db.prepare("SELECT 1 FROM expense_ai_proposals WHERE id=? AND state='running' AND lease_token=?").bind(id, lease).first()); },
    async fail(id, lease, row, actor, stale, message, inputTokens, outputTokens, usage) {
      const usageStatements = (status: "succeeded" | "failed") => expenseAiUsageStatements(db, id, lease, row, actor, usage, status);
      await db.batch([
      assertion(
        db,
        lease,
        "failed-job-lease",
        "EXISTS(SELECT 1 FROM expense_ai_proposals WHERE id=? AND state='running' AND lease_token=?)",
        [id, lease],
      ),
      db
        .prepare(
          "UPDATE expense_ai_proposals SET state=?,error=?,result_json=NULL,claim_until=NULL,lease_token=NULL,updated_at=? WHERE id=? AND lease_token=?",
        )
        .bind(stale ? "stale" : "failed", message, Date.now(), id, lease),
      db
        .prepare(
          "UPDATE expense_ai_usage SET state='failed',lease_until=0,input_tokens=?,output_tokens=? WHERE id=?",
        )
        .bind(inputTokens, outputTokens, id),
      ...usageStatements("failed"),
      workflowExpenseEffects(db).clearAssertions({commandId:lease}),
    ]);
    },
  });
}
export async function runExpenseAi(db: SqlDatabase, files: FileStore, env: BetaEnvironment, id: string, generate = generateProviderJson) {
  return nativeExpenseAiWorker(db, files, env, generate)(id);
}
export async function drainExpenseAi(db: SqlDatabase, files: FileStore, env: BetaEnvironment, expenseId?: string) {
  return createDrainExpenseAi({
    async queued(id) { return (await db.prepare("SELECT id FROM expense_ai_proposals WHERE (state='queued' OR (state='running' AND claim_until<?)) AND (?::text IS NULL OR expense_id=?) ORDER BY created_at LIMIT 2").bind(Date.now(), id ?? null, id ?? null).all<{ id: string }>()).results.map(row => row.id); },
    run: nativeExpenseAiWorker(db, files, env),
  })(expenseId);
}

export function nativeExpenseAiQueue(db: SqlDatabase, env: BetaEnvironment){return (...args:Parameters<ReturnType<typeof originalNativeExpenseAiQueue>>)=>withExpensePurchasedCommand(db,args[0],{key:args[3],purpose:"new-work"},database=>originalNativeExpenseAiQueue(database, env)(...args));}
