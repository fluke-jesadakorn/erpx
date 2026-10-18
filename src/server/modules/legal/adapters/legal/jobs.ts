import {legalQueuedPurchasePredicate,withLegalQueuedCompletion} from '../queued-purchases';
import {withLegalPurchasedCommand} from '../purchases';
import "server-only";
import type {SqlDatabase,SqlStatement} from "../../../runtime/public/runtime/types";
import type {WorkContext} from "../../../identity_access/public/authorization";
import {loadIamAuthority} from "../../../identity_access/public/iam-store";
import {legalAiActorFacts} from "../../../identity_access/public/legal-ai-actor-facts";
import type {BetaEnvironment} from "../../../manage_erpx/public/beta-config";
import {executeCommand,assertion} from "../../../workflow/public/commands";
import {resolveAiProvider,aiUsageStatement} from "../../../agent/public/ai-providers";
import {generateProviderJson,withAiGatewayScope} from "../../../agent/public/ai-transport";
import {ApiError} from "../../../runtime/public/http";
import {access,guards} from "./access";
import {hash} from "./service";
import {legalRuntimePredicate,legalCompanyCapabilityReady} from "./runtime-gate";
import {retrieveLegalAiEvidence,revalidateLegalAiEvidence,validateLegalAiCitations} from "./knowledge";
import {createLegalAiApplication} from "../../application/jobs";
import type {LegalWrite} from "../../application/service";
export {legalReminders} from "./reminders";
function application(db:SqlDatabase,a?:WorkContext,env?:BetaEnvironment,generate=generateProviderJson){const actor=()=>{if(!a)throw new Error("LEGAL_AI_COMMAND_ACTOR_REQUIRED");return a;},authority=legalAiActorFacts(db),nativeWrites=new WeakMap<LegalWrite,SqlStatement>();const stage=(statement:SqlStatement):LegalWrite=>{const write:LegalWrite={owner:"legal"};nativeWrites.set(write,statement);return write;};return createLegalAiApplication({actor,clock:{now:()=>Date.now()},hash,error:(status,code,message)=>new ApiError(status,code,message),authority:a=>loadIamAuthority(db,a.companyId,a.userId,a.role,a.authority?.sessionId),ready:id=>legalCompanyCapabilityReady(db,id,"ai"),access:(a,id,p)=>access(db,a,id,p),evidence:{retrieve:(a,id,question)=>retrieveLegalAiEvidence(db,a,id,question),revalidate:(a,evidence)=>revalidateLegalAiEvidence(db,a,evidence),validate:validateLegalAiCitations},provider:async(companyId,jobId,branchId)=>{if(!env)throw new Error("LEGAL_AI_PROVIDER_ENVIRONMENT_REQUIRED");const provider=await resolveAiProvider(db,companyId,env,"text");return {model:provider.endpoint.model,generate:(messages,maxTokens)=>generate(withAiGatewayScope(provider.endpoint,jobId,"legal",branchId),messages,maxTokens),usage:async(outcome,input,output,branch)=>{await aiUsageStatement(db,jobId,provider.snapshot,"legal",outcome,input,output,branch).run();}};},repository:{
actorAssignment:facts=>authority.actorAssignment(facts),
actorContext:facts=>authority.actorContext(facts),
aiEnabled:(facts:{companyId:string;})=>db.prepare("SELECT 1 FROM legal_settings WHERE company_id=? AND ai_enabled AND enabled").bind(facts.companyId).first(),
queueProposal:(facts:{key:string;id:string;version:number;sourceHash:string;question:string;actorId:string;now:number;actorJson:string;})=>stage(db.prepare("INSERT INTO legal_ai_proposals(id,item_id,source_version,source_hash,question,state,created_by,created_at,actor_json) VALUES(?,?,?,?,?,'queued',?,?,?)").bind(facts.key,facts.id,facts.version,facts.sourceHash,facts.question,facts.actorId,facts.now,facts.actorJson)),
history:(facts:{id:string;actorId:string;})=>db.prepare("SELECT id,source_version,question,proposal,state,error,created_at FROM legal_ai_proposals WHERE item_id=? AND created_by=? ORDER BY created_at DESC LIMIT 30").bind(facts.id,facts.actorId).all(),
interruptExpired:async (facts:{now:number;})=>{await db.prepare("UPDATE legal_ai_proposals SET state='failed',error='Generation interrupted; retry explicitly.' WHERE state='running' AND lease_until<?").bind(facts.now).run();},
claim:async(facts:{fact0:number;})=>db.prepare(`UPDATE legal_ai_proposals SET state='running',lease_until=? WHERE id=(SELECT p.id FROM legal_ai_proposals p JOIN legal_items i ON i.id=p.item_id WHERE p.state='queued' AND ${legalRuntimePredicate('i.company_id','ai')} AND ${await legalQueuedPurchasePredicate(db,'i','ai')} ORDER BY p.created_at FOR UPDATE OF p SKIP LOCKED LIMIT 1) RETURNING *`).bind(facts.fact0).first<{
      id: string;
      item_id: string;
      source_version: number;
      source_hash: string;
      question: string;
      actor_json: string;
    }>(),
assignmentLease:facts=>authority.assignmentLease(facts),
enabledBeforeGeneration:(facts:{companyId:string;})=>db.prepare("SELECT 1 FROM legal_settings WHERE company_id=? AND enabled AND ai_enabled").bind(facts.companyId).first(),
assignmentAfterGeneration:facts=>authority.assignmentAfterGeneration(facts),
enabledAfterGeneration:(facts:{companyId:string;})=>db.prepare("SELECT 1 FROM legal_settings WHERE company_id=? AND enabled AND ai_enabled").bind(facts.companyId).first(),
complete:async (facts:{fact0:"proposed" | "conflicted";fact1:string;id:string;})=>withLegalQueuedCompletion(db,"legal_ai_proposals",facts.id,async owner=>{await owner.prepare("UPDATE legal_ai_proposals SET state=?,proposal=?::jsonb WHERE id=? AND state='running'").bind(facts.fact0,facts.fact1,facts.id).run();}),
failed:async (facts:{fact0:string;id:string;})=>{await db.prepare("UPDATE legal_ai_proposals SET state='failed',error=? WHERE id=? AND state='running'").bind(facts.fact0,facts.id).run();},
requireAiLimit:(key,values)=>stage(assertion(db,key,"legal-ai-limit","(SELECT count(*) FROM legal_ai_proposals p JOIN legal_items i ON i.id=p.item_id WHERE i.company_id=? AND p.created_at>?)<10",values)),
guardChanges:(id,version,key)=>guards(db,actor(),id,version,key).map(stage),commit:(key,operation,input,id,version,result,writes)=>executeCommand(db,actor(),key,operation,input,id,version,result,writes.map(write=>{const statement=nativeWrites.get(write);if(!statement)throw new Error("LEGAL_AI_REPOSITORY_MISMATCH");return statement;})),
}});}
export function queueLegalAi(db:SqlDatabase,a:WorkContext,raw:unknown,key:string){const input=(raw&&typeof raw==="object"?raw:{}) as Record<string,unknown>;return withLegalPurchasedCommand(db,a,{key,id:typeof input.id==="string"?input.id:undefined,operation:"legal.ai.queue",purpose:"new-work",permission:"legal.ai",additional:["legal.documents"]},tx=>application(tx,a).queueLegalAi(raw,key));}
export function legalAiHistory(db:SqlDatabase,a:WorkContext,id:string){return application(db,a).legalAiHistory(id);}
export function drainLegalAi(db:SqlDatabase,env:BetaEnvironment,generate=generateProviderJson){return application(db,undefined,env,generate).drainLegalAi();}
