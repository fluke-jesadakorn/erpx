import type {WorkContext} from '@/server/modules/identity_access/public/authorization';
import type {AiProviderSnapshot} from '@/server/modules/agent/public/ai-providers';
import {minorToDecimal} from "@/lib/finance/finance-money";
import {financeAiApplySchema,financeAiPolicySchema,financeAiPrepareSchema,type FinanceAiKind,type FinanceAiResult} from '../domain/ai-contracts';
import {financeAiScope,validateFinanceAiResult,type FinanceAiInput,type FinanceAiSnapshot} from '../domain/ai-source';
import {financeAiPolicyView,type FinanceAiPolicy,type FinanceAiProposal} from '../domain/ai';
import type {FinanceError} from '../domain/invoice-line-revision';
export type FinanceAiPolicyInput=ReturnType<typeof financeAiPolicySchema.parse>;
export type FinanceAiPolicyResult={id:string;version:number;enabled:boolean};
export type FinanceAiQueueResult={id:string;state:string};
export type FinanceAiDismissResult={id:string;state:string;version:number};
export type FinanceAiDraftInput={expectedVersion?:number;aiProposalVersion?:number;debitAccountId:string;creditAccountId:string;taxAccountId?:string;aiReason?:string};
export type FinanceAiDraftPlan={actor:WorkContext;id:string;invoiceId:string;input:FinanceAiDraftInput;key:string;row:FinanceAiProposal;policy:FinanceAiPolicy;snapshot:FinanceAiSnapshot;provider:AiProviderSnapshot};
export interface FinanceAiPorts{
 error:FinanceError;permission(actor:WorkContext,permission:'finance.read'|'finance.write'|'ai.manage'):WorkContext;hasPermission(actor:WorkContext,permission:'finance.write'|'ai.manage'):boolean;errorFacts(error:unknown):{code:string|null;message:string|null};now():number;id():string;businessDate(timezone:string):string;hash(value:string):Promise<string>;captureActor(actor:WorkContext):Promise<WorkContext>;currentAuthority(actor:WorkContext):Promise<void>;enabled(actor:WorkContext):Promise<void>;provider(actor:WorkContext,kind:'text'|'image'):Promise<AiProviderSnapshot>;policy(actor:WorkContext):Promise<FinanceAiPolicy|null>;snapshot(actor:WorkContext,input:FinanceAiInput,timezone:string):Promise<FinanceAiSnapshot>;revision(companyId:string):Promise<number>;
 replay<T extends object>(actor:WorkContext,key:string,operation:string,id:string,input:unknown):Promise<T|null>;
 savePolicy(facts:{actor:WorkContext;key:string;operation:string;input:FinanceAiPolicyInput;version:number;delegate:WorkContext;now:number}):Promise<FinanceAiPolicyResult>;
 existing(actor:WorkContext,input:FinanceAiInput,snapshot:FinanceAiSnapshot,provider:AiProviderSnapshot,policy:FinanceAiPolicy):Promise<FinanceAiProposal|null>;quota(actor:WorkContext,month:string,businessDate:string):Promise<{used:number;allowance:number;daily:number;monthly:number}|null>;
 reuseProposal(facts:{actor:WorkContext;key:string;operation:string;input:FinanceAiInput;snapshot:FinanceAiSnapshot;old:FinanceAiProposal;policy:FinanceAiPolicy;provider:AiProviderSnapshot}):Promise<FinanceAiQueueResult>;
 reserve(facts:{actor:WorkContext;key:string;operation:string;input:FinanceAiInput;snapshot:FinanceAiSnapshot;old:FinanceAiProposal|null;policy:FinanceAiPolicy;provider:AiProviderSnapshot;now:number;month:string;businessDate:string;id:string;usageId:string;queuedActor:string}):Promise<FinanceAiQueueResult>;
 claim(id:string,token:string,now:number):Promise<FinanceAiProposal|null>;renewal(id:string,token:string):()=>Promise<void>;
 generate(actor:WorkContext,proposal:FinanceAiProposal,provider:AiProviderSnapshot,content:string,sessionId:string):Promise<{result:unknown;inputTokens:number;outputTokens:number}>;
 complete(facts:{actor:WorkContext;command:string;id:string;token:string;claimed:FinanceAiProposal;result:FinanceAiResult;inputTokens:number;outputTokens:number;provider:AiProviderSnapshot;latest:AiProviderSnapshot;latestPolicy:FinanceAiPolicy;current:FinanceAiSnapshot;resultHash:string}):Promise<unknown>;
 failLease(id:string,token:string,stale:boolean,message:string):Promise<object|null>;recordFailure(facts:{actor:WorkContext;id:string;claimed:FinanceAiProposal;provider:AiProviderSnapshot|null;stale:boolean;message:string;inputTokens:number;outputTokens:number}):Promise<unknown>;pending(now:number):Promise<{id:string}[]>;exhausted(now:number):Promise<unknown>;proposals(actor:WorkContext):Promise<FinanceAiProposal[]>;events(actor:WorkContext):Promise<{proposal_id:string;action:string;evidence_json:string;created_at:number}[]>;proposal(actor:WorkContext,id:string):Promise<FinanceAiProposal|null>;dismiss(facts:{actor:WorkContext;key:string;input:{expectedVersion:number;reason:string};id:string;row:FinanceAiProposal}):Promise<FinanceAiDismissResult>;draftProposal(actor:WorkContext,id:string,invoiceId:string):Promise<FinanceAiProposal|null>;invoiceProposal(actor:WorkContext,id:string):Promise<FinanceAiProposal|null>;applyInvoice(actor:WorkContext,input:Record<string,unknown>,key:string,invoiceId:string):Promise<Record<string,unknown>>;
 scanPolicies(now:number):Promise<FinanceAiPolicy[]>;claimPolicy(policy:FinanceAiPolicy,now:number):Promise<object|null>;scanInvoices(actor:WorkContext,kinds:FinanceAiKind[],day:string,policy:FinanceAiPolicy):Promise<{id:string;version:number;kind:string;status:string;due_date:string}[]>;scanBank(actor:WorkContext,policy:FinanceAiPolicy):Promise<{id:string}[]>;scanPeriods(actor:WorkContext,day:string,kind:'close_review'|'tax_review',policy:FinanceAiPolicy):Promise<{id:string}[]>;scanCharts(actor:WorkContext,day:string,policy:FinanceAiPolicy):Promise<{id:string}[]>;scanExisting(actor:WorkContext,input:FinanceAiInput,policy:FinanceAiPolicy,snapshot:FinanceAiSnapshot,day:string):Promise<object|null>;dailyExists(actor:WorkContext,day:string,policy:FinanceAiPolicy):Promise<object|null>;scanExpenses(actor:WorkContext):Promise<{id:string}[]>;expenseSource(actor:WorkContext,id:string):Promise<{evidenceCount:number;sourceVersion:number;allowed:boolean}>;queueExpense(actor:WorkContext,id:string,sourceVersion:number,policyVersion:number,key:string):Promise<unknown>;scanSucceeded(policy:FinanceAiPolicy):Promise<unknown>;scanFailed(policy:FinanceAiPolicy,message:string,nextScan:number):Promise<unknown>;
}
export function createFinanceAi(ports:FinanceAiPorts){
const scope=(actor:WorkContext)=>financeAiScope(actor,ports.permission,ports.error);
async function readFinanceAiPolicy(actor: WorkContext) {
  scope(actor);
  return await ports.policy(actor);
}

async function enabledPolicy(actor: WorkContext, kind: FinanceAiKind) {
  const policy = await readFinanceAiPolicy(actor);
  if (!policy?.enabled || !(JSON.parse(policy.kinds_json) as string[]).includes(kind)) throw ports.error(409, "FINANCE_AI_POLICY_DISABLED", "This capability is not enabled in the Company Finance AI policy. Continue manually or review Finance setup.");
  return policy;
}

async function saveFinanceAiPolicy(actor: WorkContext, raw: unknown, key: string) {
  actor = ports.permission(scope(actor), "ai.manage");
  const input = financeAiPolicySchema.parse(raw), operation = "finance.ai.policy", prior = await ports.replay<FinanceAiPolicyResult>(actor,key,operation,actor.companyId,input);
  if (prior) return prior;
  const current = await readFinanceAiPolicy(actor);
  if (Number(current?.version ?? 0) !== input.expectedVersion) throw ports.error(409, "VERSION_CONFLICT", "The Finance AI policy changed. Refresh before saving.");
  await ports.enabled(actor);
  if (input.enabled) { await ports.provider(actor,"text"); if(input.kinds.includes("receipt_extraction"))await ports.provider(actor,"image"); }
  const captured = await ports.captureActor(actor);
  // Explicit Company policy delegates advisory preparation to this existing
  // assignment. It survives a console role switch and rechecks the grant/revision.
  const delegate = { ...captured, authority: undefined, pageRequest: undefined, assignments: [], availableAssignments: [], workspaces: [], email: "" };
  const version = input.expectedVersion + 1, now = ports.now();
  return await ports.savePolicy({actor,key,operation,input,version,delegate,now});
}

async function queueFinanceAi(actor: WorkContext, raw: unknown, key: string) {
  actor = await ports.captureActor(scope(actor));
  const input = financeAiPrepareSchema.parse(raw), operation = "finance.ai.prepare";
  const prior = await ports.replay<FinanceAiQueueResult>(actor,key,operation,input.sourceId,input);
  if (prior) return prior;
  const policy = await enabledPolicy(actor, input.kind);
  await ports.enabled(actor);
  const snapshot = await ports.snapshot(actor, input, policy.timezone), provider = await ports.provider(actor,"text");
  const old = await ports.existing(actor,input,snapshot,provider,policy);
  if (old && (!input.retry || !["failed", "stale"].includes(old.state))) return await ports.reuseProposal({actor,key,operation,input,snapshot,old,policy,provider});
  const now = ports.now(), month = new Date(now).toISOString().slice(0, 7), businessDate = ports.businessDate(policy.timezone), id = old?.id ?? `finance-ai-${key}`, usageId = `finance-ai-usage-${key}`;
  const quota = await ports.quota(actor,month,businessDate);
  if (quota && (Number(quota.used) >= Number(quota.allowance) || Number(quota.daily) >= Number(policy.daily_limit) || Number(quota.monthly) >= Number(policy.monthly_limit))) throw ports.error(429, "AI_QUOTA_EXCEEDED", "The Company AI preparation allowance is reached. Your records are saved; continue manually.");
  const queuedActor = JSON.stringify({ ...actor, pageRequest: undefined, workspaces: [], assignments: [], availableAssignments: [], email: "" });
  return await ports.reserve({actor,key,operation,input,snapshot,old,policy,provider,now,month,businessDate,id,usageId,queuedActor});
}

async function runFinanceAi(id: string) {
  const token = ports.id(), now = ports.now();
  const claimed = await ports.claim(id,token,now);
  if (!claimed) return;
  let provider: AiProviderSnapshot | null = null, inputTokens = 0, outputTokens = 0;
  const actor = JSON.parse(claimed.actor_json) as WorkContext;
  const stopRenewal=ports.renewal(id,token);
  try {
    await ports.currentAuthority(actor);
    await ports.enabled(actor);
    const policy = await enabledPolicy(actor, claimed.kind);
    if (Number(policy.version) !== Number(claimed.policy_version)) throw ports.error(409, "AI_STALE", "The Company AI policy changed. Prepare a fresh proposal.");
    const input = JSON.parse(claimed.source_query_json) as FinanceAiInput, before = await ports.snapshot(actor, input, policy.timezone);
    provider = await ports.provider(actor,"text");
    if (before.hash !== claimed.source_hash || JSON.stringify(provider) !== claimed.provider_json) throw ports.error(409, "AI_STALE", "The source or provider changed. Prepare a fresh proposal.");
    const sourceContent = JSON.stringify({ kind: claimed.kind, sources: before.sources });
    if (sourceContent.length > 60000) throw ports.error(422, "AI_SOURCE_TOO_LARGE", "This evidence set is too large for a reliable proposal. Narrow the date range or review manually.");
    const output = await ports.generate(actor,claimed,provider,sourceContent,await ports.hash(`finance:${actor.companyId}:${claimed.kind}:${claimed.source_id}`));
    inputTokens = output.inputTokens; outputTokens = output.outputTokens;
    const result = validateFinanceAiResult(output.result,before,ports.error);
    await ports.currentAuthority(actor);
    const latestPolicy = await enabledPolicy(actor, claimed.kind), latest = await ports.provider(actor,"text"), current = await ports.snapshot(actor, input, latestPolicy.timezone);
    if (current.hash !== claimed.source_hash || Number(latestPolicy.version) !== Number(claimed.policy_version) || JSON.stringify(latest) !== claimed.provider_json) throw ports.error(409, "AI_STALE", "The source, policy or provider changed during preparation. Current records are preserved.");
    const command = `finance-ai-result-${token}`;
    await ports.complete({actor,command,id,token,claimed,result,inputTokens,outputTokens,provider,latest:latest,latestPolicy,current,resultHash:await ports.hash(JSON.stringify(result))});
  } catch (error) {
    const stale = ports.errorFacts(error).code!==null && ["AI_STALE", "AI_STAGE_CHANGED", "IAM_JOB_STALE", "FINANCE_AI_POLICY_DISABLED", "FEATURE_COMING_SOON", "PERMISSION_REQUIRED", "FINANCE_AI_SCOPE_REQUIRED"].includes(ports.errorFacts(error).code!);
    const message = ports.errorFacts(error).code!==null ? ports.errorFacts(error).message! : "AI could not prepare a valid proposal. Continue manually or retry.";
    const changed = await ports.failLease(id,token,stale,message);
    if (changed) await ports.recordFailure({actor,id,claimed,provider,stale,message,inputTokens,outputTokens});
  } finally { await stopRenewal(); }
}

async function drainFinanceAi() {
  const rows = await ports.pending(ports.now());
  for (const row of rows) await runFinanceAi(row.id);
  await ports.exhausted(ports.now());
}

async function financeAiWorkspace(actor: WorkContext) {
  scope(actor);
  const policy = await readFinanceAiPolicy(actor);
  let provider: AiProviderSnapshot | null = null, unavailable: string | null = null;
  try { provider = await ports.provider(actor,"text"); }
  catch (error) { unavailable = ports.errorFacts(error).code!==null ? ports.errorFacts(error).message! : "AI is temporarily unavailable. Continue manually."; }
  const revision = await ports.revision(actor.companyId);
  const rows = await ports.proposals(actor);
  const events=await ports.events(actor);
  const proposals = rows.map(row => {
    const snapshot = JSON.parse(row.source_json) as FinanceAiSnapshot;
    const current = !!policy?.enabled && Number(row.policy_version) === Number(policy.version) && !!provider && row.provider_json === JSON.stringify(provider) && Number(row.company_revision) === revision && (row.kind !== "today_brief" || row.source_id === ports.businessDate(policy.timezone));
    return { id: row.id, kind: row.kind, sourceId: row.source_id, sourceVersion: Number(row.source_version), sourceHash: row.source_hash, query:snapshot.input, version: Number(row.version), state: ["applied", "dismissed"].includes(row.state) ? row.state : current ? row.state : "stale", result: current && row.state === "ready" && row.result_json ? JSON.parse(row.result_json) as FinanceAiResult : null, error: row.error, href: snapshot.href, sourceLabel: snapshot.sources[0]?.label ?? row.source_id, sources: snapshot.sources, retainedResult: (!current||["applied","dismissed"].includes(row.state))&&row.result_json?JSON.parse(row.result_json) as FinanceAiResult:null, events:events.filter(event=>event.proposal_id===row.id).map(event=>({action:event.action,evidence:JSON.parse(event.evidence_json) as Record<string,unknown>,createdAt:Number(event.created_at)})), updatedAt: Number(row.updated_at) };
  });
  return { policy: financeAiPolicyView(policy), canManage: ports.hasPermission(actor, "ai.manage"), canWrite: ports.hasPermission(actor, "finance.write"), unavailable, providerModel: provider?.model ?? null, proposals };
}

async function dismissFinanceAi(actor: WorkContext, id: string, input: { expectedVersion: number; reason: string }, key: string) {
  scope(actor);
  const prior = await ports.replay<FinanceAiDismissResult>(actor,key,"finance.ai.dismiss",id,input); if (prior) return prior;
  const row = await ports.proposal(actor,id);
  if (!row) throw ports.error(404, "PROPOSAL_NOT_FOUND", "Proposal not found in this Company.");
  if (Number(row.version) !== input.expectedVersion || row.state === "applied") throw ports.error(409, "VERSION_CONFLICT", "The proposal changed. Refresh before recording a decision.");
  return await ports.dismiss({actor,key,input,id,row});
}

async function financeAiDraftApplication(actor: WorkContext, id: string, invoiceId: string, input: { expectedVersion?: number; aiProposalVersion?: number; debitAccountId: string; creditAccountId: string; taxAccountId?: string; aiReason?: string }, key: string): Promise<FinanceAiDraftPlan> {
  actor = ports.permission(scope(actor), "finance.write");
  const row = await ports.draftProposal(actor,id,invoiceId);
  if (!row || row.state !== "ready" || Number(row.version) !== input.aiProposalVersion || !row.result_json || !JSON.parse(row.result_json).accounts || Number(row.source_version) !== input.expectedVersion || !input.aiReason || input.aiReason.trim().length < 10) throw ports.error(409, "AI_STALE", "Review a current document proposal and provide a reason before saving its draft.");
  const policy = await enabledPolicy(actor, row.kind), snapshot = await ports.snapshot(actor, JSON.parse(row.source_query_json), policy.timezone), provider = await ports.provider(actor,"text");
  if (snapshot.hash !== row.source_hash || Number(policy.version) !== Number(row.policy_version) || JSON.stringify(provider) !== row.provider_json) throw ports.error(409, "AI_STALE", "The source, policy or provider changed. Prepare a new proposal or continue manually.");
  return {actor,id,invoiceId,input,key,row,policy,snapshot,provider};}

async function applyFinanceAiDraft(actor: WorkContext, id: string, raw: unknown, key: string) {
  const input = financeAiApplySchema.parse(raw);
  ports.permission(scope(actor), "finance.write");
  const row = await ports.invoiceProposal(actor,id);
  if (!row) throw ports.error(404, "PROPOSAL_NOT_FOUND", "Document proposal not found in this Company.");
  const source = (JSON.parse(row.source_json) as FinanceAiSnapshot).sources[0]?.data;
  if (!source || source.source_type !== "standalone" || source.status !== "draft") throw ports.error(409, "AI_STAGE_CHANGED", "Use the source workflow for this document. AI coding saves standalone drafts only.");
  return ports.applyInvoice(actor, { expectedVersion: Number(row.source_version), partyId: source.party_id, kind: source.kind, invoiceNumber: source.invoice_number, invoiceDate: source.invoice_date, dueDate: source.due_date, currency: source.currency, subtotal: minorToDecimal(Number(source.subtotal_minor)), tax: minorToDecimal(Number(source.tax_minor)), taxCode: source.tax_code ?? undefined, ...input.accounts, taxAccountId: input.accounts.taxAccountId ?? undefined, aiProposalId: id, aiProposalVersion: input.expectedVersion, aiReason: input.reason }, key, row.source_id);
}

async function scanFinanceAi() {
  const now = ports.now();
  const policies = await ports.scanPolicies(now);
  for (const policy of policies) {
    const claimed = await ports.claimPolicy(policy,now);
    if (!claimed) continue;
    const actor = JSON.parse(policy.owner_actor_json) as WorkContext, kinds = JSON.parse(policy.kinds_json) as FinanceAiKind[];
    let queued = 0;
    try {
      await ports.currentAuthority(actor); await ports.enabled(actor);
      const day = ports.businessDate(policy.timezone), candidates: FinanceAiInput[] = [];
      if (kinds.includes("today_brief")) candidates.push({kind:"today_brief",sourceId:day,retry:false});
      const invoices = await ports.scanInvoices(actor,kinds,day,policy);
      for (const row of invoices) {
        const kind = row.status === "draft" ? "invoice_review" : row.kind === "receivable" ? "collection_draft" : "payment_preparation";
        if (kinds.includes(kind) && (kind !== "payment_preparation" || row.due_date <= day)) candidates.push({kind,sourceId:row.id,retry:false});
      }
      if (kinds.includes("bank_match")) for (const row of await ports.scanBank(actor,policy)) candidates.push({kind:"bank_match",sourceId:row.id,retry:false});
      for (const kind of ["close_review","tax_review"] as const) if (kinds.includes(kind)) {
        const periods = await ports.scanPeriods(actor,day,kind,policy);
        for (const row of periods) candidates.push({kind,sourceId:row.id,retry:false});
      }
      if (kinds.includes("report_explanation")) for (const row of await ports.scanCharts(actor,day,policy)) candidates.push({kind:"report_explanation",sourceId:row.id,retry:false,startDate:`${day.slice(0,7)}-01`,endDate:day});
      for (const input of candidates) {
        if (queued >= 2) break;
        try {
          const snapshot = await ports.snapshot(actor,input,policy.timezone);
          // Automatic preparation runs once per record version (reports once per
          // business date). People can explicitly request fresh changed evidence.
          const exists = await ports.scanExisting(actor,input,policy,snapshot,day);
          const dailyExists = input.kind === "today_brief" && await ports.dailyExists(actor,day,policy);
          if (exists || dailyExists) continue;
          await queueFinanceAi(actor,input,ports.id()); queued++;
        } catch (error) {
          if (ports.errorFacts(error).code!==null && ["STATUTORY_PROFILE_REQUIRED","AI_STAGE_CHANGED","CHART_NOT_FOUND"].includes(ports.errorFacts(error).code!)) continue;
          throw error;
        }
      }
      if (queued < 2 && kinds.includes("receipt_extraction")) {
        const expenses = await ports.scanExpenses(actor);
        for (const row of expenses) {
          if (queued >= 2) break;
          const source=await ports.expenseSource(actor,row.id);
          if (!source.evidenceCount || !source.allowed) continue;
          await ports.queueExpense(actor,row.id,source.sourceVersion,Number(policy.version),ports.id()); queued++;
        }
      }
      await ports.scanSucceeded(policy);
    } catch (error) {
      const message = ports.errorFacts(error).code!==null ? ports.errorFacts(error).message! : "Automatic preparation is unavailable. Continue manually and review the Company AI policy.";
      await ports.scanFailed(policy,message,ports.now()+15*60000);
    }
  }
}
return {readFinanceAiPolicy,saveFinanceAiPolicy,queueFinanceAi,runFinanceAi,drainFinanceAi,financeAiWorkspace,dismissFinanceAi,financeAiDraftApplication,applyFinanceAiDraft,scanFinanceAi};}
