import { nativeOwnerWrite, nativeOwnerWriteNoopSql } from './native-owner-write';
import { agentSelectColsFromSql } from './read-tail-r26-source-sql';
import { agentAssistantRateGuardSql } from './operations-guided-intake-facts';
import { agentTurnEvidenceSql } from './turn-evidence-projection-source';
import { assistantQuotaSql } from './assistant-quota-sql';
import {clearAgentBootstrapAssertion} from "@erpx/workflow-server/public/agent-bootstrap-assertions";
import {aiRequestQuotaLock} from "@erpx/agent-server/adapters/ai-quota";
import "server-only";

import {type assistantAskSchema} from "@erpx/work-projection-ui/lib/ai/workspace-assistant";
import type {z} from "zod";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import type {BetaEnvironment} from "@erpx/manage-erpx-server/public/beta-config";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import {sha256} from "@erpx/platform-runtime-server/public/crypto";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {executeCommand} from '@erpx/workflow-server/public/commands';
import {workContextGuards} from '@erpx/workflow-server/public/command-guards';
import {iamMutationGuards} from "@erpx/identity-access-server/public/iam-store";
import {aiProviderOverview,aiSnapshotGuard,aiUsageStatement,resolveAiProvider} from "@erpx/agent-server/adapters/ai-runtime";
import {generateProviderJson,withAiGatewayScope} from "@erpx/agent-server/adapters/ai-transport";
import {employeeRequestDetail} from '@erpx/records-server/public/employee-requests-read';
import {getExpense} from '@erpx/people-payroll-server/public/expense-read';
import {getTask} from '@erpx/operations-server/public/staff-task-read';
import {nativeHandbookReads} from "@erpx/collaboration-ui/lib/native-handbook-page";
import {runtimeEnv} from "../../../../../platform/runtime/server/src/public/runtime";
import {assistantGovernanceContext,assistantGovernanceGuards,validateAssistantGovernance,recordAssistantInvocation} from "@erpx/agent-server/adapters/agent-governance";
import {pmsWorkspace} from "../../../../pms/server/src/public/pms";
import {posWorkspace} from "@erpx/pos-server/public/pos";
import {commercePage} from '@erpx/commerce-server/public/commerce';
import {financeOverviewCounts} from '@erpx/finance-server/public/finance';
import {closeWorkspace} from "@erpx/finance-server/public/finance-workspace";
import { assertProductionModuleEnabled } from "@erpx/pms-server/public/feature-availability";
import {getWorkPage} from "@erpx/work-projection-server/public/work-summary";
import {homePosts,postView} from '@erpx/collaboration-server/public/social-home';
import {getLeave} from '@erpx/people-payroll-server/public/leave-read';

import type {SqlStatement} from '@erpx/platform-runtime-server/public/runtime/types';
import {BusinessRuleError} from '@erpx/platform-shared-server/index';
import {createWorkspaceAssistantApplication,type AssistantRepository} from '@erpx/agent-server/application/assistant';
import {type Row} from '@erpx/agent-server/domain/assistant';
import type {AiEndpoint} from '@erpx/agent-server/adapters/ai-transport';
const agentWorkspaceTurnByIdSql = (cols:string) => agentSelectColsFromSql(cols, 'ai_workspace_turns WHERE id=?');
async function exposed<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
async function inward<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof ApiError)throw new BusinessRuleError(error.status,error.code,error.message);throw error;}}
function bindRepository(db:SqlDatabase):AssistantRepository<SqlStatement>{return {clearAuthorityAssertions(key:string,companyId:string){return clearAgentBootstrapAssertion(db,key,companyId);},
async conversationTurns(companyId:string,userId:string,assignmentId:string,value:string,key:string,sessionId:string){return (await db.prepare(`${agentTurnEvidenceSql("t.id,t.question,t.paths_json,t.sources_json,t.answer_json,t.english_answer_json,t.state,t.created_at,e.governance_snapshot_json")} WHERE t.company_id=? AND t.user_id=? AND t.assignment_id=? AND t.scope_key=? AND t.conversation_key=? AND t.private_session_id=? AND t.state='ready' ORDER BY t.created_at DESC LIMIT 20`)
    .bind(companyId,userId,assignmentId,value,key,sessionId).all<Row>()).results;},
async previousTurn(id:string){return db.prepare(agentWorkspaceTurnByIdSql("*")).bind(id).first<Row&{user_id:string;assignment_id:string;scope_key:string;company_id:string}>();},
guardRate(id:string,companyId:string,value:number){return assertion(db,id,"assistant-rate",agentAssistantRateGuardSql,[companyId,value]);},
guardQuota(id:string,companyId:string,value:number,value2:string){return assertion(db,id,"assistant-quota",`${assistantQuotaSql('ai_calls')}`,[companyId,value,companyId,value2,companyId,value2,companyId,value2,companyId]);},
async prepareTurn(id:string,companyId:string,userId:string,assignmentId:string,value:string,conversationKey:string,question:string,value2:string,now:number,sessionId:string){return nativeOwnerWrite('ag_workspace_turn_insert',[id,companyId,userId,assignmentId,value,conversationKey,question,value2,now,now,sessionId],`ag-turn:${id}`,{companyId,now},()=>db.prepare(nativeOwnerWriteNoopSql));},
clearReservationAssertions(id:string,companyId:string){return clearAgentBootstrapAssertion(db,id,companyId);},
async prepareAnswer(value:string,sourcesJson:string,now:number,id:string){return nativeOwnerWrite('ag_workspace_turn_ready',[value,sourcesJson,now,id],`ag-turn-ready:${id}`,{now},()=>db.prepare(nativeOwnerWriteNoopSql));},
async prepareFailedTurn(value:string,now:number,id:string){return nativeOwnerWrite('ag_workspace_turn_failed',[value,now,id],`ag-turn-failed:${id}`,{now},()=>db.prepare(nativeOwnerWriteNoopSql));},
async translationTurn(id:string){return db.prepare(agentWorkspaceTurnByIdSql("state,answer_json")).bind(id).first<Row>();},
guardTranslationQuota(id:string,companyId:string,value:number,month:string){return assertion(db,id,"translation-quota",`${assistantQuotaSql('calls')}`,[companyId,value,companyId,month,companyId,month,companyId,month,companyId]);},
async prepareTranslation(id:string,companyId:string,userId:string,assignmentId:string,value:string,value2:string,value3:"Translate the previous AI response for English display.",value4:string,now:number,sessionId:string){return nativeOwnerWrite('ag_workspace_turn_insert',[id,companyId,userId,assignmentId,value,value2,value3,value4,now,now,sessionId],`ag-turn:${id}`,{companyId,now},()=>db.prepare(nativeOwnerWriteNoopSql));},
clearTranslationAssertions(id:string,companyId:string){return clearAgentBootstrapAssertion(db,id,companyId);},
async prepareEnglishDisplay(value:string,id:string,companyId:string,userId:string){return nativeOwnerWrite('ag_workspace_turn_english',[value,id,companyId,userId],`ag-turn-english:${id}`,{companyId},()=>db.prepare(nativeOwnerWriteNoopSql));},
async prepareTranslationAnswer(value:string,now:number,id:string){return nativeOwnerWrite('ag_workspace_turn_translation_ready',[value,now,id],`ag-turn-translation:${id}`,{now},()=>db.prepare(nativeOwnerWriteNoopSql));},
async prepareFailedTranslation(now:number,id:string){return nativeOwnerWrite('ag_workspace_turn_translation_failed',[now,id],`ag-turn-translation-failed:${id}`,{now},()=>db.prepare(nativeOwnerWriteNoopSql));},async verifyAuthority(actor,key){await db.batch([...workContextGuards(db,actor,key),...iamMutationGuards(db,actor,key),await clearAgentBootstrapAssertion(db,key,actor.companyId)]);},commit:(...args)=>executeCommand(db,...args),async batch(effects){await db.batch(effects);},quotaLock:companyId=>aiRequestQuotaLock(db,companyId),snapshotGuard:(key,snapshot)=>aiSnapshotGuard(db,key,snapshot),usage:async(...args)=>aiUsageStatement(db,...args),governanceGuards:(...args)=>assistantGovernanceGuards(db,...args)};}
function application(db:SqlDatabase,env?:BetaEnvironment,generate=generateProviderJson){return createWorkspaceAssistantApplication({repository:bindRepository(db),sources:{employeeRequestDetail:(...args)=>inward(()=>employeeRequestDetail(db,...args)),getExpense:(...args)=>inward(()=>getExpense(...args)),getTask:(...args)=>inward(()=>getTask(db,...args)),handbookArticle:async(actor,id)=>inward(async()=>(await nativeHandbookReads(actor,actor.holdingSlug)).handbookArticle(actor,id)),pmsWorkspace:(...args)=>inward(()=>pmsWorkspace(...args)),posWorkspace:(...args)=>inward(()=>posWorkspace(...args)),commercePage:(...args)=>inward(()=>commercePage(...args)),financeOverviewCounts:(...args)=>inward(()=>financeOverviewCounts(...args)),closeWorkspace:(...args)=>inward(()=>closeWorkspace(...args)),assertProductionModuleEnabled:(...args)=>inward(()=>assertProductionModuleEnabled(db,...args)),getWorkPage:(...args)=>inward(()=>getWorkPage(...args)),homePosts:(...args)=>inward(()=>homePosts(db,...args)),postView:(...args)=>inward(()=>postView(db,...args)),getLeave:(...args)=>inward(()=>getLeave(...args))},clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},hash:sha256,production:process.env.NODE_ENV==='production',provider:actor=>inward(()=>resolveAiProvider(db,actor.companyId,env!,"text",actor.holdingSlug)),providerOverview:actor=>inward(async()=>aiProviderOverview(db,actor,await runtimeEnv())),bindGateway:(endpoint,...args)=>withAiGatewayScope(endpoint as AiEndpoint,...args),generate:(endpoint,...args)=>inward(()=>generate(endpoint as AiEndpoint,...args)),generateTranslation:(endpoint,...args)=>inward(()=>generateProviderJson(endpoint as AiEndpoint,...args)),governance:{context:(...args)=>inward(()=>assistantGovernanceContext(db,...args)),validate:(...args)=>inward(()=>validateAssistantGovernance(db,...args)),record:(...args)=>inward(()=>recordAssistantInvocation(db,...args))}});}
export async function askAssistant(db:SqlDatabase,actor:WorkContext,env:BetaEnvironment,input:z.infer<typeof assistantAskSchema>,id:string,generate=generateProviderJson){return exposed(()=>application(db,env,generate).askAssistant(actor,input,id));}

export async function assistantView(db:SqlDatabase,actor:WorkContext,env:BetaEnvironment,path:string,mode:"page"|"workspace"){return exposed(()=>application(db,env).assistantView(actor,path,mode));}
