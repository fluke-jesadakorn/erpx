import { clearWorkflowCommandAssertions } from "@erpx/server-workflow/public/command-assertion-effects";
import {financeAiAgentUsageSql} from '@erpx/server-agent/public/finance-ai-quota-sources';
import {financeAiSocialUsageSql,financeAiCompanyAllowanceSql} from '@erpx/server-manage_erpx/public/finance-ai-quota-sources';
import {financeAiExpenseUsageSql,financeAiExpenseMonthlyReservationsSql,financeAiExpenseDatedReservationsSql,financeAiExpenseDailyReservationsSql} from '@erpx/server-people_payroll/public/finance-ai-quota-sources';
import {financeAiRequestUsageSql} from '@erpx/server-workflow/public/finance-ai-request-sources';
import {aiRequestQuotaLock} from "@erpx/server-agent/public/ai-quota";
import "server-only";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { financeAiDate } from "@erpx/feature-finance/lib/finance-ai";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { BetaEnvironment } from "@erpx/server-manage_erpx/public/beta-config";
import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import { captureJobAuthority, requireJobAuthority } from "@erpx/server-identity_access/public/iam-jobs";
import { iamMutationGuards } from "@erpx/server-identity_access/public/iam-store";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { resolveAiProvider, aiSnapshotGuard, aiUsageStatement } from "@erpx/server-agent/public/ai-providers";
import { generateProviderJson, withAiGatewayScope } from "@erpx/server-agent/public/ai-transport";
import { assertProductionModuleEnabled, productionFeatureGuard } from "@erpx/server-pms/public/hospitality-rollout";
import { financeAiRevision, financeAiSnapshot, financeRevisionSql, requireFinanceAiScope } from "@erpx/server-finance/public/finance-ai-source";
import { ApiError } from "@erpx/server-runtime/public/http";
import { sha256 } from "@erpx/server-runtime/public/crypto";

import {createFinanceAi,type FinanceAiDraftInput,type FinanceAiDraftPlan} from '@erpx/server-finance/application/ai';
import {systemPrompt,type FinanceAiPolicy,type FinanceAiProposal} from '@erpx/server-finance/domain/ai';
import {financeAiReceiptCandidates} from '@erpx/server-people_payroll/public/finance-ai-receipt-candidates';
export {financeAiPolicyView} from '@erpx/server-finance/domain/ai';
export type {FinanceAiPolicy,FinanceAiProposal} from '@erpx/server-finance/domain/ai';
const event = (db: SqlDatabase, actor: WorkContext, proposalId: string | null, action: string, evidence: unknown, commandId: string | null) => db.prepare("INSERT INTO finance_ai_events(id,company_id,proposal_id,action,actor_id,command_id,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(), actor.companyId, proposalId, action, actor.userId, commandId, JSON.stringify(evidence), Date.now());
function policyGuard(db: SqlDatabase, key: string, policy: FinanceAiPolicy) { return assertion(db, key, "finance-ai-policy", "EXISTS(SELECT 1 FROM finance_ai_policies WHERE company_id=? AND enabled AND version=?)", [policy.company_id, policy.version]); }
async function currentAuthority(db: SqlDatabase, actor: WorkContext) {
  requireJobAuthority(actor); requireFinanceAiScope(actor);
  const key = crypto.randomUUID();
  try { await db.batch([...iamMutationGuards(db, actor, key), clearWorkflowCommandAssertions(db, key)]); }
  catch { throw new ApiError(409, "IAM_JOB_STALE", "The authorizing assignment or access policy changed. Review the Company AI policy."); }
}
// Every reservation, including an explicit retry, consumes a shared Company request.
export const financeAiQuotaSql = `(SELECT count(*) FROM (
  ${financeAiSocialUsageSql} UNION ALL ${financeAiExpenseUsageSql}
  UNION ALL ${financeAiRequestUsageSql} UNION ALL SELECT company_id,month FROM finance_ai_usage
  UNION ALL ${financeAiAgentUsageSql}
) calls WHERE company_id=? AND month=?)`;
const financePolicyTimezoneSql="(SELECT timezone FROM finance_ai_policies WHERE company_id=expense_ai_proposals.company_id)";


function nativeFinanceAi(db:SqlDatabase,env:BetaEnvironment,generate=generateProviderJson){const providers=new Map<string,Awaited<ReturnType<typeof resolveAiProvider>>>();return {...createFinanceAi({error:(...args)=>new ApiError(...args),permission:requirePermission,hasPermission,errorFacts:error=>error instanceof ApiError?{code:error.code,message:error.message}:{code:null,message:null},now:Date.now,id:()=>crypto.randomUUID(),businessDate:financeAiDate,hash:sha256,captureActor:actor=>captureJobAuthority(db,actor),currentAuthority:actor=>currentAuthority(db,actor),enabled:actor=>assertProductionModuleEnabled(db,actor.companyId,'finance'),provider:async(actor,kind)=>{const p=await resolveAiProvider(db,actor.companyId,env,kind,actor.holdingSlug);providers.set(JSON.stringify(p.snapshot),p);return p.snapshot;},snapshot:(actor,input,timezone)=>financeAiSnapshot(db,actor,input,timezone),revision:companyId=>financeAiRevision(db,companyId),replay:(actor,key,operation,id,input)=>replayCommand(db,actor,key,operation,id,input),renewal:(id,token)=>{  let renewing:Promise<unknown>|null=null;
  const renewal=setInterval(()=>{if(renewing)return;renewing=db.prepare("UPDATE finance_ai_proposals SET claim_until=? WHERE id=? AND state='running' AND lease_token=? AND claim_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint").bind(Date.now()+120000,id,token).run().catch(()=>{}).finally(()=>{renewing=null;});},30000);
return async()=>{clearInterval(renewal);if(renewing)await renewing;};},generate:async(actor,claimed,snapshot,sourceContent,sessionId)=>{const provider=providers.get(JSON.stringify(snapshot))!;return generate({...withAiGatewayScope(provider.endpoint,`${claimed.usage_id}:${claimed.attempts}`,'finance_proposal',null),sessionId},[{role:'system',content:systemPrompt},{role:'user',content:sourceContent}],2500)},applyInvoice:async(actor,input,key,id)=>{const {createInvoice,invoiceSchema}=await import('@erpx/server-finance/public/finance');return createInvoice(actor,invoiceSchema.parse(input),key,id)},scanExpenses:actor=>financeAiReceiptCandidates(db,actor.companyId),expenseSource:async(actor,id)=>{const {expenseAiSource,expenseAiAllowed}=await import('@erpx/server-people_payroll/public/expense-ai-source');const s=await expenseAiSource(db,actor.companyId,id,null);return {evidenceCount:s.evidence.length,sourceVersion:s.sourceVersion,allowed:s.evidence.length?await expenseAiAllowed(db,actor,s.claim,null):false}},queueExpense:async(actor,id,sourceVersion,policyVersion,key)=>{const {queueExpenseAi}=await import('@erpx/server-people_payroll/public/expense-ai');return queueExpenseAi(db,actor,env,id,{paymentId:null,expectedVersion:sourceVersion,retry:false,financePolicyVersion:policyVersion},key)},policy:async(actor)=>db.prepare("SELECT * FROM finance_ai_policies WHERE company_id=?").bind(actor.companyId).first<FinanceAiPolicy>(),
existing:async(actor,input,snapshot,provider,policy)=>db.prepare("SELECT * FROM finance_ai_proposals WHERE company_id=? AND kind=? AND source_id=? AND source_hash=? AND provider_json=? AND policy_version=?").bind(actor.companyId, input.kind, input.sourceId, snapshot.hash, JSON.stringify(provider), policy.version).first<FinanceAiProposal>(),
quota:async(actor,month,businessDate)=>db.prepare(`SELECT ${financeAiQuotaSql} used,${financeAiCompanyAllowanceSql} allowance,(SELECT count(*) FROM (SELECT company_id,business_date,month FROM finance_ai_usage UNION ALL ${financeAiExpenseDatedReservationsSql(financePolicyTimezoneSql)}) reservations WHERE company_id=? AND business_date=?) daily,(SELECT count(*) FROM (SELECT company_id,month FROM finance_ai_usage UNION ALL ${financeAiExpenseMonthlyReservationsSql}) reservations WHERE company_id=? AND month=?) monthly`).bind(actor.companyId, month, actor.companyId, actor.companyId, businessDate, actor.companyId, month).first<{ used: number; allowance: number; daily: number; monthly: number }>(),
claim:async(id,token,now)=>db.prepare("UPDATE finance_ai_proposals SET state='running',lease_token=?,claim_until=?,attempts=attempts+1,updated_at=? WHERE id=? AND attempts<3 AND (state='queued' OR (state='running' AND claim_until<?)) RETURNING *").bind(token, now + 120000, now, id, now).first<FinanceAiProposal>(),
savePolicy:async(facts)=>{const {actor,key,operation,input,version,delegate,now}=facts;return executeCommand(db, actor, key, operation, input, actor.companyId, version, { id: actor.companyId, version, enabled: input.enabled }, [
    ...productionFeatureGuard(db, key, actor.companyId, "finance"),
    assertion(db, key, "finance-ai-policy-version", "COALESCE((SELECT version FROM finance_ai_policies WHERE company_id=?),0)=?", [actor.companyId, input.expectedVersion]),
    db.prepare("INSERT INTO finance_ai_policies(company_id,enabled,version,kinds_json,timezone,daily_limit,monthly_limit,owner_actor_json,reason,changed_by,changed_at,next_scan_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,0) ON CONFLICT(company_id) DO UPDATE SET enabled=excluded.enabled,version=excluded.version,kinds_json=excluded.kinds_json,timezone=excluded.timezone,daily_limit=excluded.daily_limit,monthly_limit=excluded.monthly_limit,owner_actor_json=excluded.owner_actor_json,reason=excluded.reason,changed_by=excluded.changed_by,changed_at=excluded.changed_at,next_scan_at=0,last_error=NULL").bind(actor.companyId, input.enabled, version, JSON.stringify(input.kinds), input.timezone, input.dailyLimit, input.monthlyLimit, JSON.stringify(delegate), input.reason, actor.userId, now),
    event(db, actor, null, "policy", { enabled: input.enabled, kinds: input.kinds, version, reason: input.reason, ownerAssignment: actor.assignmentId }, key),
  ]);},
reuseProposal:async(facts)=>{const {actor,key,operation,input,snapshot,old,policy,provider}=facts;return executeCommand(db, actor, key, operation, input, input.sourceId, snapshot.version, { id: old.id, state: old.state }, [policyGuard(db, key, policy), aiSnapshotGuard(db, key, provider)]);},
reserve:async(facts)=>{const {actor,key,operation,input,snapshot,old,policy,provider,now,month,businessDate,id,usageId,queuedActor}=facts;return executeCommand(db, actor, key, operation, input, input.sourceId, snapshot.version, { id, state: "queued" }, [
    db.prepare("SELECT company_id FROM finance_ai_policies WHERE company_id=? FOR UPDATE").bind(actor.companyId), aiRequestQuotaLock(db,actor.companyId),
    ...productionFeatureGuard(db, key, actor.companyId, "finance"), policyGuard(db, key, policy), aiSnapshotGuard(db, key, provider),
    assertion(db, key, "finance-ai-source", `${financeRevisionSql}=?`, [actor.companyId, snapshot.companyRevision]),
    assertion(db, key, "finance-ai-budget", `${financeAiQuotaSql}<${financeAiCompanyAllowanceSql} AND (SELECT count(*) FROM (SELECT company_id,business_date FROM finance_ai_usage UNION ALL ${financeAiExpenseDailyReservationsSql(financePolicyTimezoneSql)}) reservations WHERE company_id=? AND business_date=?)<? AND (SELECT count(*) FROM (SELECT company_id,month FROM finance_ai_usage UNION ALL ${financeAiExpenseMonthlyReservationsSql}) reservations WHERE company_id=? AND month=?)<? AND (SELECT count(*) FROM finance_ai_usage WHERE company_id=? AND state='reserved' AND lease_until>?)<2`, [actor.companyId, month, actor.companyId, actor.companyId, businessDate, policy.daily_limit, actor.companyId, month, policy.monthly_limit, actor.companyId, now]),
    old ? db.prepare("UPDATE finance_ai_proposals SET state='queued',version=version+1,error=NULL,attempts=0,lease_token=NULL,claim_until=NULL,actor_json=?,usage_id=?,updated_at=? WHERE id=? AND state IN ('failed','stale')").bind(queuedActor, usageId, now, id) : db.prepare("INSERT INTO finance_ai_proposals(id,company_id,kind,source_id,source_version,source_hash,source_json,source_query_json,company_revision,policy_version,provider_json,actor_json,state,usage_id,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,'queued',?,?,?)").bind(id, actor.companyId, input.kind, input.sourceId, snapshot.version, snapshot.hash, JSON.stringify(snapshot), JSON.stringify(snapshot.input), snapshot.companyRevision, policy.version, JSON.stringify(provider), queuedActor, usageId, now, now),
    db.prepare("INSERT INTO finance_ai_usage(id,proposal_id,company_id,actor_id,month,business_date,state,lease_until,created_at) VALUES(?,?,?,?,?,?,'reserved',?,?)").bind(usageId, id, actor.companyId, actor.userId, month, businessDate, now + 15 * 60000, now),
    event(db, actor, id, old ? "retry" : "prepare", { sourceHash: snapshot.hash, sourceVersion: snapshot.version, policyVersion: policy.version, provider: provider, usageId }, key),
  ]);},
complete:async(facts)=>{const {actor,command,id,token,claimed,result,inputTokens,outputTokens,provider,latest,latestPolicy,current,resultHash}=facts;return db.batch([
      ...iamMutationGuards(db, actor, command), ...productionFeatureGuard(db, command, actor.companyId, "finance"), policyGuard(db, command, latestPolicy), aiSnapshotGuard(db, command, latest),
      assertion(db, command, "finance-ai-source", `${financeRevisionSql}=?`, [actor.companyId, current.companyRevision]),
      assertion(db, command, "finance-ai-lease", "EXISTS(SELECT 1 FROM finance_ai_proposals WHERE id=? AND state='running' AND lease_token=? AND claim_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint)", [id, token]),
      db.prepare("UPDATE finance_ai_proposals SET state='ready',result_json=?,error=NULL,lease_token=NULL,claim_until=NULL,version=version+1,updated_at=? WHERE id=? AND lease_token=?").bind(JSON.stringify(result), Date.now(), id, token),
      db.prepare("UPDATE finance_ai_usage SET state='succeeded',lease_until=0,input_tokens=?,output_tokens=? WHERE id=?").bind(inputTokens, outputTokens, claimed.usage_id),
      aiUsageStatement(db, `${claimed.usage_id}:${claimed.attempts}`, provider, "finance_proposal", "succeeded", inputTokens, outputTokens),
      event(db, actor, id, "ready", { sourceHash: claimed.source_hash, sourceVersion: claimed.source_version, provider: latest, resultHash: resultHash }, null),
      clearWorkflowCommandAssertions(db, command),
    ]);},
failLease:async(id,token,stale,message)=>db.prepare("UPDATE finance_ai_proposals SET state=?,error=?,lease_token=NULL,claim_until=NULL,version=version+1,updated_at=? WHERE id=? AND state='running' AND lease_token=? RETURNING id").bind(stale ? "stale" : "failed", message, Date.now(), id, token).first(),
recordFailure:async(facts)=>{const {actor,id,claimed,provider,stale,message,inputTokens,outputTokens}=facts;return db.batch([
      db.prepare("UPDATE finance_ai_usage SET state='failed',lease_until=0,input_tokens=?,output_tokens=? WHERE id=?").bind(inputTokens, outputTokens, claimed.usage_id),
      ...(provider ? [aiUsageStatement(db, `${claimed.usage_id}:${claimed.attempts}`, provider, "finance_proposal", "failed", inputTokens, outputTokens)] : []),
      event(db, actor, id, stale ? "stale" : "failed", { message, attempts: claimed.attempts }, null),
    ]);},
pending:async(now)=>(await db.prepare("SELECT id FROM finance_ai_proposals WHERE attempts<3 AND (state='queued' OR (state='running' AND claim_until<?)) ORDER BY created_at,id LIMIT 2").bind(now).all<{ id: string }>()).results,
exhausted:async(now)=>{return db.batch([
    db.prepare("UPDATE finance_ai_proposals SET state='failed',error='Preparation was interrupted. Retry or continue manually.',lease_token=NULL,claim_until=NULL,version=version+1,updated_at=? WHERE state='running' AND claim_until<? AND attempts>=3").bind(now, now),
    db.prepare("UPDATE finance_ai_usage SET state='failed',lease_until=0 WHERE state='reserved' AND proposal_id IN (SELECT id FROM finance_ai_proposals WHERE state IN ('failed','stale','dismissed'))"),
  ]);},
proposals:async(actor)=>(await db.prepare("SELECT * FROM finance_ai_proposals WHERE company_id=? ORDER BY updated_at DESC,id LIMIT 30").bind(actor.companyId).all<FinanceAiProposal>()).results,
events:async(actor)=>(await db.prepare("SELECT proposal_id,action,evidence_json,created_at FROM finance_ai_events WHERE company_id=? AND proposal_id IS NOT NULL ORDER BY created_at DESC LIMIT 120").bind(actor.companyId).all<{proposal_id:string;action:string;evidence_json:string;created_at:number}>()).results,
proposal:async(actor,id)=>db.prepare("SELECT * FROM finance_ai_proposals WHERE id=? AND company_id=?").bind(id, actor.companyId).first<FinanceAiProposal>(),
dismiss:async(facts)=>{const {actor,key,input,id,row}=facts;return executeCommand(db, actor, key, "finance.ai.dismiss", input, id, input.expectedVersion + 1, { id, state: "dismissed", version: input.expectedVersion + 1 }, [
    assertion(db, key, "finance-ai-proposal", "EXISTS(SELECT 1 FROM finance_ai_proposals WHERE id=? AND company_id=? AND version=? AND state<>'applied')", [id, actor.companyId, input.expectedVersion]),
    db.prepare("UPDATE finance_ai_proposals SET state='dismissed',version=version+1,lease_token=NULL,claim_until=NULL,updated_at=? WHERE id=?").bind(Date.now(), id),
    event(db, actor, id, "dismiss", { reason: input.reason, sourceHash: row.source_hash }, key),
  ]);},
draftProposal:async(actor,id,invoiceId)=>db.prepare("SELECT * FROM finance_ai_proposals WHERE id=? AND company_id=? AND kind='invoice_review' AND source_id=?").bind(id, actor.companyId, invoiceId).first<FinanceAiProposal>(),
invoiceProposal:async(actor,id)=>db.prepare("SELECT * FROM finance_ai_proposals WHERE id=? AND company_id=? AND kind='invoice_review'").bind(id, actor.companyId).first<FinanceAiProposal>(),
scanPolicies:async(now)=>(await db.prepare("SELECT * FROM finance_ai_policies WHERE enabled AND next_scan_at<=? ORDER BY next_scan_at,company_id LIMIT 3").bind(now).all<FinanceAiPolicy>()).results,
claimPolicy:async(policy,now)=>db.prepare("UPDATE finance_ai_policies SET next_scan_at=? WHERE company_id=? AND enabled AND version=? AND next_scan_at<=? RETURNING company_id").bind(now + 60000,policy.company_id,policy.version,now).first(),
scanInvoices:async(actor,kinds,day,policy)=>(await db.prepare("SELECT id,version,kind,status,due_date FROM finance_invoices WHERE company_id=? AND ((status='draft' AND ?::boolean) OR (status IN ('posted','partially_paid') AND outstanding_minor>0 AND ((kind='receivable' AND ?::boolean) OR (kind='payable' AND due_date<=? AND ?::boolean)))) AND NOT EXISTS(SELECT 1 FROM finance_ai_proposals p WHERE p.company_id=finance_invoices.company_id AND p.source_id=finance_invoices.id AND p.kind=CASE WHEN finance_invoices.status='draft' THEN 'invoice_review' WHEN finance_invoices.kind='receivable' THEN 'collection_draft' ELSE 'payment_preparation' END AND p.policy_version=? AND p.source_version=finance_invoices.version) ORDER BY due_date,id LIMIT 30").bind(actor.companyId,kinds.includes("invoice_review"),kinds.includes("collection_draft"),day,kinds.includes("payment_preparation"),policy.version).all<{id:string;version:number;kind:string;status:string;due_date:string}>()).results,
scanBank:async(actor,policy)=>(await db.prepare("SELECT id FROM finance_bank_transactions WHERE company_id=? AND match_status<>'matched' AND NOT EXISTS(SELECT 1 FROM finance_ai_proposals p WHERE p.company_id=finance_bank_transactions.company_id AND p.source_id=finance_bank_transactions.id AND p.kind='bank_match' AND p.policy_version=? AND p.source_version=finance_bank_transactions.version) ORDER BY transaction_date,id LIMIT 20").bind(actor.companyId,policy.version).all<{id:string}>()).results,
scanPeriods:async(actor,day,kind,policy)=>(await db.prepare("SELECT id FROM finance_periods WHERE company_id=? AND (state='closing' OR (state='open' AND end_date<?)) AND NOT EXISTS(SELECT 1 FROM finance_ai_proposals p WHERE p.company_id=finance_periods.company_id AND p.source_id=finance_periods.id AND p.kind=? AND p.policy_version=? AND p.source_version=finance_periods.version) ORDER BY end_date,id LIMIT 3").bind(actor.companyId,day,kind,policy.version).all<{id:string}>()).results,
scanCharts:async(actor,day,policy)=>(await db.prepare("SELECT id FROM finance_charts WHERE company_id=? AND NOT EXISTS(SELECT 1 FROM finance_ai_proposals p WHERE p.company_id=finance_charts.company_id AND p.source_id=finance_charts.id AND p.kind='report_explanation' AND p.policy_version=? AND p.source_query_json::jsonb->>'endDate'=?) ORDER BY id LIMIT 5").bind(actor.companyId,policy.version,day).all<{id:string}>()).results,
scanExisting:async(actor,input,policy,snapshot,day)=>db.prepare("SELECT id FROM finance_ai_proposals WHERE company_id=? AND kind=? AND source_id=? AND policy_version=? AND (?::boolean OR source_version=?) AND (?::text IS NULL OR source_query_json::jsonb->>'endDate'=?) LIMIT 1").bind(actor.companyId,input.kind,input.sourceId,policy.version,input.kind==="report_explanation",snapshot.version,input.kind==="report_explanation"?day:null,input.kind==="report_explanation"?day:null).first(),
dailyExists:async(actor,day,policy)=>db.prepare("SELECT id FROM finance_ai_proposals WHERE company_id=? AND kind='today_brief' AND source_id=? AND policy_version=? LIMIT 1").bind(actor.companyId,day,policy.version).first(),
scanSucceeded:async(policy)=>{return db.prepare("UPDATE finance_ai_policies SET last_error=NULL WHERE company_id=? AND version=?").bind(policy.company_id,policy.version).run();},
scanFailed:async(policy,message,nextScan)=>{return db.prepare("UPDATE finance_ai_policies SET last_error=?,next_scan_at=? WHERE company_id=? AND version=?").bind(message,nextScan,policy.company_id,policy.version).run();}}),draftStatements:(facts:FinanceAiDraftPlan)=>{const {actor,id,invoiceId,input,key,row,policy,snapshot,provider}=facts;  return [
    policyGuard(db, key, policy), aiSnapshotGuard(db, key, provider),
    // executeCommand inserts this invoice-update receipt before domain steps.
    assertion(db, key, "finance-ai-source", `${financeRevisionSql}=?`, [actor.companyId, snapshot.companyRevision + 1]),
    assertion(db, key, "finance-ai-proposal", "EXISTS(SELECT 1 FROM finance_ai_proposals WHERE id=? AND company_id=? AND state='ready' AND version=?)", [id, actor.companyId, row.version]),
    db.prepare("UPDATE finance_ai_proposals SET state='applied',draft_command_id=?,version=version+1,updated_at=? WHERE id=?").bind(key, Date.now(), id),
    event(db, actor, id, "apply_draft", { invoiceId, sourceHash: row.source_hash, sourceVersion: row.source_version, selectedAccounts: { debitAccountId: input.debitAccountId, creditAccountId: input.creditAccountId, taxAccountId: input.taxAccountId ?? null }, reason: input.aiReason }, key),
  ];
}};}
export const readFinanceAiPolicy=(db:SqlDatabase,actor:WorkContext)=>nativeFinanceAi(db,{}).readFinanceAiPolicy(actor);
export const saveFinanceAiPolicy=(db:SqlDatabase,actor:WorkContext,env:BetaEnvironment,raw:unknown,key:string)=>nativeFinanceAi(db,env).saveFinanceAiPolicy(actor,raw,key);
export const queueFinanceAi=(db:SqlDatabase,actor:WorkContext,env:BetaEnvironment,raw:unknown,key:string)=>nativeFinanceAi(db,env).queueFinanceAi(actor,raw,key);
export const runFinanceAi=(db:SqlDatabase,env:BetaEnvironment,id:string,generate=generateProviderJson)=>nativeFinanceAi(db,env,generate).runFinanceAi(id);
export const drainFinanceAi=(db:SqlDatabase,env:BetaEnvironment)=>nativeFinanceAi(db,env).drainFinanceAi();
export const financeAiWorkspace=(db:SqlDatabase,actor:WorkContext,env:BetaEnvironment)=>nativeFinanceAi(db,env).financeAiWorkspace(actor);
export const dismissFinanceAi=(db:SqlDatabase,actor:WorkContext,id:string,input:{expectedVersion:number;reason:string},key:string)=>nativeFinanceAi(db,{}).dismissFinanceAi(actor,id,input,key);
export async function financeAiDraftApplication(db:SqlDatabase,actor:WorkContext,env:BetaEnvironment,id:string,invoiceId:string,input:FinanceAiDraftInput,key:string):Promise<SqlStatement[]>{const native=nativeFinanceAi(db,env);return native.draftStatements(await native.financeAiDraftApplication(actor,id,invoiceId,input,key));}
export const applyFinanceAiDraft=(db:SqlDatabase,actor:WorkContext,id:string,raw:unknown,key:string)=>nativeFinanceAi(db,{}).applyFinanceAiDraft(actor,id,raw,key);
export const scanFinanceAi=(db:SqlDatabase,env:BetaEnvironment)=>nativeFinanceAi(db,env).scanFinanceAi();
