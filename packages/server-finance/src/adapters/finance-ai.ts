import { AI_USAGE_MONTH_SQL } from './finance-read-tail-r44-sql';
import { finPeriodIdByCompanySql } from './finance-read-tail-r57-sql';
import { financeAiProposalPointSql, financeReferenceListSql } from './finance-read-tail-r42-sql';
import { clearWorkflowCommandAssertions } from "@erpx/server-workflow/public/command-assertion-effects";
import { nativeFinanceOwnerWriteSystem } from './native-finance-owner-write';
import { nativeFinanceReadAll, nativeFinanceReadFirst, FINANCE_RD_AI_POLICY, FINANCE_RD_AI_PROPOSAL_EXACT, FINANCE_RD_AI_CLAIMABLE, FINANCE_RD_AI_EVENTS, FINANCE_RD_AI_DUE_POLICIES, FINANCE_RD_AI_BANK_CANDIDATES, FINANCE_RD_AI_REPORT_CHARTS, FINANCE_RD_AI_PROPOSAL_BY_SOURCE } from './native-finance-read';
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
import { assertProductionModuleEnabled, productionFeatureGuard } from "@erpx/server-pms/public/feature-availability";
import { financeAiRevision, financeAiSnapshot, financeRevisionSql, requireFinanceAiScope } from "@erpx/server-finance/public/finance-ai-source";
import { ApiError } from "@erpx/server-runtime/public/http";
import { sha256 } from "@erpx/server-runtime/public/crypto";

import {createFinanceAi,type FinanceAiDraftInput,type FinanceAiDraftPlan} from '@erpx/server-finance/application/ai';
import {systemPrompt,type FinanceAiPolicy,type FinanceAiProposal} from '@erpx/server-finance/domain/ai';
import {financeAiReceiptCandidates} from '@erpx/server-people_payroll/public/finance-ai-receipt-candidates';
export {financeAiPolicyView} from '@erpx/server-finance/domain/ai';
export type {FinanceAiPolicy,FinanceAiProposal} from '@erpx/server-finance/domain/ai';
const event = async (db: SqlDatabase, actor: WorkContext, proposalId: string | null, action: string, evidence: unknown, commandId: string | null) => await nativeFinanceOwnerWriteSystem(db, actor.companyId, 'ai_stmt_1', [crypto.randomUUID(), actor.companyId, proposalId, action, actor.userId, commandId, JSON.stringify(evidence), Date.now()]);
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
  UNION ALL ${financeAiRequestUsageSql} UNION ALL ${AI_USAGE_MONTH_SQL}
  UNION ALL ${financeAiAgentUsageSql}
) calls WHERE company_id=? AND month=?)`;
const financePolicyTimezoneSql="(SELECT timezone FROM finance_ai_policies WHERE company_id=expense_ai_proposals.company_id)";


function nativeFinanceAi(db:SqlDatabase,env:BetaEnvironment,generate=generateProviderJson){const providers=new Map<string,Awaited<ReturnType<typeof resolveAiProvider>>>();return {...createFinanceAi({error:(...args)=>new ApiError(...args),permission:requirePermission,hasPermission,errorFacts:error=>error instanceof ApiError?{code:error.code,message:error.message}:{code:null,message:null},now:Date.now,id:()=>crypto.randomUUID(),businessDate:financeAiDate,hash:sha256,captureActor:actor=>captureJobAuthority(db,actor),currentAuthority:actor=>currentAuthority(db,actor),enabled:actor=>assertProductionModuleEnabled(db,actor.companyId,'finance'),provider:async(actor,kind)=>{const p=await resolveAiProvider(db,actor.companyId,env,kind,actor.holdingSlug);providers.set(JSON.stringify(p.snapshot),p);return p.snapshot;},snapshot:(actor,input,timezone)=>financeAiSnapshot(db,actor,input,timezone),revision:companyId=>financeAiRevision(db,companyId),replay:(actor,key,operation,id,input)=>replayCommand(db,actor,key,operation,id,input),renewal:(id,token)=>{  let renewing:Promise<unknown>|null=null;
  const renewal=setInterval(()=>{if(renewing)return;renewing=nativeFinanceOwnerWriteSystem(db, 'system', 'ai_stmt_2', [Date.now()+120000,id,token]).then(statement=>statement.run()).catch(()=>{}).finally(()=>{renewing=null;});},30000);
return async()=>{clearInterval(renewal);if(renewing)await renewing;};},generate:async(actor,claimed,snapshot,sourceContent,sessionId)=>{const provider=providers.get(JSON.stringify(snapshot))!;return generate({...withAiGatewayScope(provider.endpoint,`${claimed.usage_id}:${claimed.attempts}`,'finance_proposal',null),sessionId},[{role:'system',content:systemPrompt},{role:'user',content:sourceContent}],2500)},applyInvoice:async(actor,input,key,id)=>{const {createInvoice,invoiceSchema}=await import('@erpx/server-finance/public/finance');return createInvoice(actor,invoiceSchema.parse(input),key,id)},scanExpenses:actor=>financeAiReceiptCandidates(db,actor.companyId),expenseSource:async(actor,id)=>{const {expenseAiSource,expenseAiAllowed}=await import('@erpx/server-people_payroll/public/expense-ai-source');const s=await expenseAiSource(db,actor.companyId,id,null);return {evidenceCount:s.evidence.length,sourceVersion:s.sourceVersion,allowed:s.evidence.length?await expenseAiAllowed(db,actor,s.claim,null):false}},queueExpense:async(actor,id,sourceVersion,policyVersion,key)=>{const {queueExpenseAi}=await import('@erpx/server-people_payroll/public/expense-ai');return queueExpenseAi(db,actor,env,id,{paymentId:null,expectedVersion:sourceVersion,retry:false,financePolicyVersion:policyVersion},key)},policy:async(actor)=>nativeFinanceReadFirst<FinanceAiPolicy>(FINANCE_RD_AI_POLICY, [actor.companyId], db),
existing:async(actor,input,snapshot,provider,policy)=>nativeFinanceReadFirst<FinanceAiProposal>(FINANCE_RD_AI_PROPOSAL_EXACT, [actor.companyId, input.kind, input.sourceId, snapshot.hash, JSON.stringify(provider), policy.version], db),
quota:async(actor,month,businessDate)=>db.prepare(`SELECT ${financeAiQuotaSql} used,${financeAiCompanyAllowanceSql} allowance,(SELECT count(*) FROM (SELECT company_id,business_date,month FROM finance_ai_usage UNION ALL ${financeAiExpenseDatedReservationsSql(financePolicyTimezoneSql)}) reservations WHERE company_id=? AND business_date=?) daily,(SELECT count(*) FROM (${AI_USAGE_MONTH_SQL} UNION ALL ${financeAiExpenseMonthlyReservationsSql}) reservations WHERE company_id=? AND month=?) monthly`).bind(actor.companyId, month, actor.companyId, actor.companyId, businessDate, actor.companyId, month).first<{ used: number; allowance: number; daily: number; monthly: number }>(),
claim:async(id,token,now)=>(await nativeFinanceOwnerWriteSystem(db, 'system', 'ai_stmt_3', [token, now + 120000, now, id, now])).first<FinanceAiProposal>(),
savePolicy:async(facts)=>{const {actor,key,operation,input,version,delegate,now}=facts;return executeCommand(db, actor, key, operation, input, actor.companyId, version, { id: actor.companyId, version, enabled: input.enabled }, [
    ...productionFeatureGuard(db, key, actor.companyId, "finance"),
    assertion(db, key, "finance-ai-policy-version", "COALESCE((SELECT version FROM finance_ai_policies WHERE company_id=?),0)=?", [actor.companyId, input.expectedVersion]),
    await nativeFinanceOwnerWriteSystem(db, actor.companyId, 'ai_stmt_4', [actor.companyId, input.enabled, version, JSON.stringify(input.kinds), input.timezone, input.dailyLimit, input.monthlyLimit, JSON.stringify(delegate), input.reason, actor.userId, now]),
    await event(db, actor, null, "policy", { enabled: input.enabled, kinds: input.kinds, version, reason: input.reason, ownerAssignment: actor.assignmentId }, key),
  ]);},
reuseProposal:async(facts)=>{const {actor,key,operation,input,snapshot,old,policy,provider}=facts;return executeCommand(db, actor, key, operation, input, input.sourceId, snapshot.version, { id: old.id, state: old.state }, [policyGuard(db, key, policy), aiSnapshotGuard(db, key, provider)]);},
reserve:async(facts)=>{const {actor,key,operation,input,snapshot,old,policy,provider,now,month,businessDate,id,usageId,queuedActor}=facts;return executeCommand(db, actor, key, operation, input, input.sourceId, snapshot.version, { id, state: "queued" }, [
    db.prepare("SELECT company_id FROM finance_ai_policies WHERE company_id=? FOR UPDATE").bind(actor.companyId), aiRequestQuotaLock(db,actor.companyId),
    ...productionFeatureGuard(db, key, actor.companyId, "finance"), policyGuard(db, key, policy), aiSnapshotGuard(db, key, provider),
    assertion(db, key, "finance-ai-source", `${financeRevisionSql}=?`, [actor.companyId, snapshot.companyRevision]),
    assertion(db, key, "finance-ai-budget", `${financeAiQuotaSql}<${financeAiCompanyAllowanceSql} AND (SELECT count(*) FROM (SELECT company_id,business_date FROM finance_ai_usage UNION ALL ${financeAiExpenseDailyReservationsSql(financePolicyTimezoneSql)}) reservations WHERE company_id=? AND business_date=?)<? AND (SELECT count(*) FROM (SELECT company_id,month FROM finance_ai_usage UNION ALL ${financeAiExpenseMonthlyReservationsSql}) reservations WHERE company_id=? AND month=?)<? AND (SELECT count(*) FROM finance_ai_usage WHERE company_id=? AND state='reserved' AND lease_until>?)<2`, [actor.companyId, month, actor.companyId, actor.companyId, businessDate, policy.daily_limit, actor.companyId, month, policy.monthly_limit, actor.companyId, now]),
    old ? await nativeFinanceOwnerWriteSystem(db, actor.companyId, 'ai_stmt_5', [queuedActor, usageId, now, id]) : await nativeFinanceOwnerWriteSystem(db, actor.companyId, 'ai_stmt_6', [id, actor.companyId, input.kind, input.sourceId, snapshot.version, snapshot.hash, JSON.stringify(snapshot), JSON.stringify(snapshot.input), snapshot.companyRevision, policy.version, JSON.stringify(provider), queuedActor, usageId, now, now]),
    await nativeFinanceOwnerWriteSystem(db, actor.companyId, 'ai_stmt_7', [usageId, id, actor.companyId, actor.userId, month, businessDate, now + 15 * 60000, now]),
    await event(db, actor, id, old ? "retry" : "prepare", { sourceHash: snapshot.hash, sourceVersion: snapshot.version, policyVersion: policy.version, provider: provider, usageId }, key),
  ]);},
complete:async(facts)=>{const {actor,command,id,token,claimed,result,inputTokens,outputTokens,provider,latest,latestPolicy,current,resultHash}=facts;return db.batch(await Promise.all([
      ...iamMutationGuards(db, actor, command), ...productionFeatureGuard(db, command, actor.companyId, "finance"), policyGuard(db, command, latestPolicy), aiSnapshotGuard(db, command, latest),
      assertion(db, command, "finance-ai-source", `${financeRevisionSql}=?`, [actor.companyId, current.companyRevision]),
      assertion(db, command, "finance-ai-lease", "EXISTS(SELECT 1 FROM finance_ai_proposals WHERE id=? AND state='running' AND lease_token=? AND claim_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint)", [id, token]),
      await nativeFinanceOwnerWriteSystem(db, actor.companyId, 'ai_stmt_8', [JSON.stringify(result), Date.now(), id, token]),
      await nativeFinanceOwnerWriteSystem(db, actor.companyId, 'ai_stmt_9', [inputTokens, outputTokens, claimed.usage_id]),
      aiUsageStatement(db, `${claimed.usage_id}:${claimed.attempts}`, provider, "finance_proposal", "succeeded", inputTokens, outputTokens),
      await event(db, actor, id, "ready", { sourceHash: claimed.source_hash, sourceVersion: claimed.source_version, provider: latest, resultHash: resultHash }, null),
      clearWorkflowCommandAssertions(db, command),
    ]));},
failLease:async(id,token,stale,message)=>(await nativeFinanceOwnerWriteSystem(db, 'system', 'ai_stmt_10', [stale ? "stale" : "failed", message, Date.now(), id, token])).first(),
recordFailure:async(facts)=>{const {actor,id,claimed,provider,stale,message,inputTokens,outputTokens}=facts;return db.batch(await Promise.all([
      await nativeFinanceOwnerWriteSystem(db, actor.companyId, 'ai_stmt_11', [inputTokens, outputTokens, claimed.usage_id]),
      ...(provider ? [aiUsageStatement(db, `${claimed.usage_id}:${claimed.attempts}`, provider, "finance_proposal", "failed", inputTokens, outputTokens)] : []),
      await event(db, actor, id, stale ? "stale" : "failed", { message, attempts: claimed.attempts }, null),
    ]));},
pending:async(now)=>(await nativeFinanceReadAll<{ id: string }>(FINANCE_RD_AI_CLAIMABLE, [now], db)).results,
exhausted:async(now)=>{return db.batch([
    await nativeFinanceOwnerWriteSystem(db, 'system', 'ai_stmt_12', [now, now]),
    await nativeFinanceOwnerWriteSystem(db, 'system', 'ai_stmt_18', [])
  ]);},
proposals:async(actor)=>(await db.prepare(financeReferenceListSql('finance_ai_proposals','company_id=?','updated_at DESC,id LIMIT 30')).bind(actor.companyId).all<FinanceAiProposal>()).results,
events:async(actor)=>(await nativeFinanceReadAll<{proposal_id:string;action:string;evidence_json:string;created_at:number}>(FINANCE_RD_AI_EVENTS, [actor.companyId], db)).results,
proposal:async(actor,id)=>db.prepare(financeAiProposalPointSql('')).bind(id, actor.companyId).first<FinanceAiProposal>(),
dismiss:async(facts)=>{const {actor,key,input,id,row}=facts;return executeCommand(db, actor, key, "finance.ai.dismiss", input, id, input.expectedVersion + 1, { id, state: "dismissed", version: input.expectedVersion + 1 }, [
    assertion(db, key, "finance-ai-proposal", "EXISTS(SELECT 1 FROM finance_ai_proposals WHERE id=? AND company_id=? AND version=? AND state<>'applied')", [id, actor.companyId, input.expectedVersion]),
    await nativeFinanceOwnerWriteSystem(db, actor.companyId, 'ai_stmt_13', [Date.now(), id]),
    await event(db, actor, id, "dismiss", { reason: input.reason, sourceHash: row.source_hash }, key),
  ]);},
draftProposal:async(actor,id,invoiceId)=>db.prepare(financeAiProposalPointSql(" AND kind='invoice_review' AND source_id=?")).bind(id, actor.companyId, invoiceId).first<FinanceAiProposal>(),
invoiceProposal:async(actor,id)=>db.prepare(financeAiProposalPointSql(" AND kind='invoice_review'")).bind(id, actor.companyId).first<FinanceAiProposal>(),
scanPolicies:async(now)=>(await nativeFinanceReadAll<FinanceAiPolicy>(FINANCE_RD_AI_DUE_POLICIES, [now], db)).results,
claimPolicy:async(policy,now)=>(await nativeFinanceOwnerWriteSystem(db, policy.company_id, 'ai_stmt_14', [now + 60000,policy.company_id,policy.version,now])).first(),
scanInvoices:async(actor,kinds,day,policy)=>(await db.prepare("SELECT id,version,kind,status,due_date FROM finance_invoices WHERE company_id=? AND ((status='draft' AND ?::boolean) OR (status IN ('posted','partially_paid') AND outstanding_minor>0 AND ((kind='receivable' AND ?::boolean) OR (kind='payable' AND due_date<=? AND ?::boolean)))) AND NOT EXISTS(SELECT 1 FROM finance_ai_proposals p WHERE p.company_id=finance_invoices.company_id AND p.source_id=finance_invoices.id AND p.kind=CASE WHEN finance_invoices.status='draft' THEN 'invoice_review' WHEN finance_invoices.kind='receivable' THEN 'collection_draft' ELSE 'payment_preparation' END AND p.policy_version=? AND p.source_version=finance_invoices.version) ORDER BY due_date,id LIMIT 30").bind(actor.companyId,kinds.includes("invoice_review"),kinds.includes("collection_draft"),day,kinds.includes("payment_preparation"),policy.version).all<{id:string;version:number;kind:string;status:string;due_date:string}>()).results,
scanBank:async(actor,policy)=>(await nativeFinanceReadAll<{id:string}>(FINANCE_RD_AI_BANK_CANDIDATES, [actor.companyId,policy.version], db)).results,
scanPeriods:async(actor,day,kind,policy)=>(await db.prepare(finPeriodIdByCompanySql(" AND (state='closing' OR (state='open' AND end_date<?)) AND NOT EXISTS(SELECT 1 FROM finance_ai_proposals p WHERE p.company_id=finance_periods.company_id AND p.source_id=finance_periods.id AND p.kind=? AND p.policy_version=? AND p.source_version=finance_periods.version) ORDER BY end_date,id LIMIT 3")).bind(actor.companyId,day,kind,policy.version).all<{id:string}>()).results,
scanCharts:async(actor,day,policy)=>(await nativeFinanceReadAll<{id:string}>(FINANCE_RD_AI_REPORT_CHARTS, [actor.companyId,policy.version,day], db)).results,
scanExisting:async(actor,input,policy,snapshot,day)=>nativeFinanceReadFirst(FINANCE_RD_AI_PROPOSAL_BY_SOURCE, [actor.companyId,input.kind,input.sourceId,policy.version,input.kind==="report_explanation",snapshot.version,input.kind==="report_explanation"?day:null,input.kind==="report_explanation"?day:null], db),
dailyExists:async(actor,day,policy)=>db.prepare("SELECT id FROM finance_ai_proposals WHERE company_id=? AND kind='today_brief' AND source_id=? AND policy_version=? LIMIT 1").bind(actor.companyId,day,policy.version).first(),
scanSucceeded:async(policy)=>{return (await nativeFinanceOwnerWriteSystem(db, policy.company_id, 'ai_stmt_15', [policy.company_id,policy.version])).run();},
scanFailed:async(policy,message,nextScan)=>{return (await nativeFinanceOwnerWriteSystem(db, policy.company_id, 'ai_stmt_16', [message,nextScan,policy.company_id,policy.version])).run();}}),draftStatements:async(facts:FinanceAiDraftPlan)=>{const {actor,id,invoiceId,input,key,row,policy,snapshot,provider}=facts;  return [
    policyGuard(db, key, policy), aiSnapshotGuard(db, key, provider),
    // executeCommand inserts this invoice-update receipt before domain steps.
    assertion(db, key, "finance-ai-source", `${financeRevisionSql}=?`, [actor.companyId, snapshot.companyRevision + 1]),
    assertion(db, key, "finance-ai-proposal", "EXISTS(SELECT 1 FROM finance_ai_proposals WHERE id=? AND company_id=? AND state='ready' AND version=?)", [id, actor.companyId, row.version]),
    await nativeFinanceOwnerWriteSystem(db, actor.companyId, 'ai_stmt_17', [key, Date.now(), id]),
    await event(db, actor, id, "apply_draft", { invoiceId, sourceHash: row.source_hash, sourceVersion: row.source_version, selectedAccounts: { debitAccountId: input.debitAccountId, creditAccountId: input.creditAccountId, taxAccountId: input.taxAccountId ?? null }, reason: input.aiReason }, key),
  ];
}};}
export const readFinanceAiPolicy=(db:SqlDatabase,actor:WorkContext)=>nativeFinanceAi(db,{}).readFinanceAiPolicy(actor);
export const saveFinanceAiPolicy=(db:SqlDatabase,actor:WorkContext,env:BetaEnvironment,raw:unknown,key:string)=>nativeFinanceAi(db,env).saveFinanceAiPolicy(actor,raw,key);
export const queueFinanceAi=(db:SqlDatabase,actor:WorkContext,env:BetaEnvironment,raw:unknown,key:string)=>nativeFinanceAi(db,env).queueFinanceAi(actor,raw,key);
export const runFinanceAi=(db:SqlDatabase,env:BetaEnvironment,id:string,generate=generateProviderJson)=>nativeFinanceAi(db,env,generate).runFinanceAi(id);
export const drainFinanceAi=(db:SqlDatabase,env:BetaEnvironment)=>nativeFinanceAi(db,env).drainFinanceAi();
export const financeAiWorkspace=(db:SqlDatabase,actor:WorkContext,env:BetaEnvironment)=>nativeFinanceAi(db,env).financeAiWorkspace(actor);
export const dismissFinanceAi=(db:SqlDatabase,actor:WorkContext,id:string,input:{expectedVersion:number;reason:string},key:string)=>nativeFinanceAi(db,{}).dismissFinanceAi(actor,id,input,key);
export async function financeAiDraftApplication(db:SqlDatabase,actor:WorkContext,env:BetaEnvironment,id:string,invoiceId:string,input:FinanceAiDraftInput,key:string):Promise<SqlStatement[]>{const native=nativeFinanceAi(db,env);return await native.draftStatements(await native.financeAiDraftApplication(actor,id,invoiceId,input,key));}
export const applyFinanceAiDraft=(db:SqlDatabase,actor:WorkContext,id:string,raw:unknown,key:string)=>nativeFinanceAi(db,{}).applyFinanceAiDraft(actor,id,raw,key);
export const scanFinanceAi=(db:SqlDatabase,env:BetaEnvironment)=>nativeFinanceAi(db,env).scanFinanceAi();
