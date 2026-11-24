import {legalQueuedPurchasePredicate,withLegalQueuedCompletion} from '@erpx/server-legal/adapters/queued-purchases';
import {nativeWorkflowWrite} from '@erpx/server-workflow/public/native-workflow-evidence';
import {withLegalPurchasedCommand} from '@erpx/server-legal/adapters/purchases';
import "server-only";
import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {loadIamAuthority} from "@erpx/server-identity_access/public/iam-store";
import {legalAiActorFacts} from "@erpx/server-identity_access/public/legal-ai-actor-facts";
import type {BetaEnvironment} from "@erpx/server-manage_erpx/public/beta-config";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand} from "@erpx/server-workflow/public/commands";
import {resolveAiProvider,aiUsageStatement} from "@erpx/server-agent/public/ai-providers";
import {generateProviderJson,withAiGatewayScope} from "@erpx/server-agent/public/ai-transport";
import {ApiError} from "@erpx/server-runtime/public/http";
import {access,guards} from "@erpx/server-legal/adapters/legal/access";
import {hash} from "@erpx/server-legal/adapters/legal/service";
import {legalRuntimePredicate,legalCompanyCapabilityReady} from "@erpx/server-legal/adapters/legal/runtime-gate";
import {retrieveLegalAiEvidence,revalidateLegalAiEvidence,validateLegalAiCitations} from "@erpx/server-legal/adapters/legal/knowledge";
import {createLegalAiApplication} from "@erpx/server-legal/application/jobs";
import type {LegalWrite} from "@erpx/server-legal/application/service";
import {legalWave3Statement} from "../native-legal-wave3";
import {nativeLegalAuxiliaryRead} from "../native-legal-auxiliary";
import {legalSettingsEnabledSql} from "../legal-read-tail-r9-sources";
export {legalReminders} from "@erpx/server-legal/adapters/legal/reminders";
/** Byte-identical AI-enabled guard; both generation checks prepare and run it. */
const aiEnabledSql = `${legalSettingsEnabledSql} AND ai_enabled`;
function application(db:SqlDatabase,a?:WorkContext,env?:BetaEnvironment,generate=generateProviderJson){const actor=()=>{if(!a)throw new Error("LEGAL_AI_COMMAND_ACTOR_REQUIRED");return a;},authority=legalAiActorFacts(db),nativeWrites=new WeakMap<LegalWrite,SqlStatement>();const stage=(statement:SqlStatement):LegalWrite=>{const write:LegalWrite={owner:"legal"};nativeWrites.set(write,statement);return write;};return createLegalAiApplication({actor,clock:{now:()=>Date.now()},hash,error:(status,code,message)=>new ApiError(status,code,message),authority:a=>loadIamAuthority(db,a.companyId,a.userId,a.role,a.authority?.sessionId),ready:id=>legalCompanyCapabilityReady(db,id,"ai"),access:(a,id,p)=>access(db,a,id,p),evidence:{retrieve:(a,id,question)=>retrieveLegalAiEvidence(db,a,id,question),revalidate:(a,evidence)=>revalidateLegalAiEvidence(db,a,evidence),validate:validateLegalAiCitations},provider:async(companyId,jobId,branchId)=>{if(!env)throw new Error("LEGAL_AI_PROVIDER_ENVIRONMENT_REQUIRED");const provider=await resolveAiProvider(db,companyId,env,"text");return {model:provider.endpoint.model,generate:(messages,maxTokens)=>generate(withAiGatewayScope(provider.endpoint,jobId,"legal",branchId),messages,maxTokens),usage:async(outcome,input,output,branch)=>{await (await aiUsageStatement(db,jobId,provider.snapshot,"legal",outcome,input,output,branch)).run();}};},repository:{
actorAssignment:facts=>authority.actorAssignment(facts),
actorContext:facts=>authority.actorContext(facts),
aiEnabled:(facts:{companyId:string;})=>db.prepare("SELECT 1 FROM legal_settings WHERE company_id=? AND ai_enabled AND enabled").bind(facts.companyId).first(),
queueProposal:(facts:{key:string;id:string;version:number;sourceHash:string;question:string;actorId:string;now:number;actorJson:string;})=>stage(nativeWorkflowWrite('legal_ai_queue',[facts.key,facts.id,facts.version,facts.sourceHash,facts.question,facts.actorId,facts.now,facts.actorJson])),
history:async (facts:{id:string;actorId:string;})=>({results:await nativeLegalAuxiliaryRead<Record<string,unknown>[]>(actor(),'ai',facts.id)}),
interruptExpired:async (facts:{now:number;})=>{await (await legalWave3Statement(db,{kind:'system'},'legal_ai_interrupt',[facts.now],`legal-ai-interrupt:${facts.now}`,facts.now)).run();},
claim:async(facts:{fact0:number;})=>(await legalWave3Statement(db,{kind:'system'},'legal_ai_claim',[facts.fact0,`${legalRuntimePredicate('i.company_id','ai')} AND ${await legalQueuedPurchasePredicate(db,'i','ai')}`],`legal-ai-claim:${facts.fact0}`,Date.now())).first<{
      id: string;
      item_id: string;
      source_version: number;
      source_hash: string;
      question: string;
      actor_json: string;
    }>(),
assignmentLease:facts=>authority.assignmentLease(facts),
enabledBeforeGeneration:(facts:{companyId:string;})=>db.prepare(aiEnabledSql).bind(facts.companyId).first(),
assignmentAfterGeneration:facts=>authority.assignmentAfterGeneration(facts),
enabledAfterGeneration:(facts:{companyId:string;})=>db.prepare(aiEnabledSql).bind(facts.companyId).first(),
complete:async (facts:{fact0:"proposed" | "conflicted";fact1:string;id:string;})=>withLegalQueuedCompletion(db,"legal_ai_proposals",facts.id,async owner=>{await (await legalWave3Statement(owner,{kind:'system'},'legal_ai_complete',[facts.fact0,facts.fact1,facts.id],`legal-ai-complete:${facts.id}`,Date.now())).run();}),
failed:async (facts:{fact0:string;id:string;})=>{await (await legalWave3Statement(db,{kind:'system'},'legal_ai_fail',[facts.fact0,facts.id],`legal-ai-fail:${facts.id}`,Date.now())).run();},
requireAiLimit:(key,values)=>stage(assertion(db,key,"legal-ai-limit","(SELECT count(*) FROM legal_ai_proposals p JOIN legal_items i ON i.id=p.item_id WHERE i.company_id=? AND p.created_at>?)<10",values)),
guardChanges:(id,version,key)=>guards(db,actor(),id,version,key).map(stage),commit:(key,operation,input,id,version,result,writes)=>executeCommand(db,actor(),key,operation,input,id,version,result,writes.map(write=>{const statement=nativeWrites.get(write);if(!statement)throw new Error("LEGAL_AI_REPOSITORY_MISMATCH");return statement;})),
}});}
export function queueLegalAi(db:SqlDatabase,a:WorkContext,raw:unknown,key:string){const input=(raw&&typeof raw==="object"?raw:{}) as Record<string,unknown>;return withLegalPurchasedCommand(db,a,{key,id:typeof input.id==="string"?input.id:undefined,operation:"legal.ai.queue",purpose:"new-work",permission:"legal.ai",additional:["legal.documents"]},tx=>application(tx,a).queueLegalAi(raw,key));}
export function legalAiHistory(db:SqlDatabase,a:WorkContext,id:string){return application(db,a).legalAiHistory(id);}
export function drainLegalAi(db:SqlDatabase,env:BetaEnvironment,generate=generateProviderJson){return application(db,undefined,env,generate).drainLegalAi();}
