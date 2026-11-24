import { nativeOwnerWrite, nativeOwnerWriteNoopSql } from './native-owner-write';
import { agentAssistantRateGuardSql } from './operations-guided-intake-facts';
import { agentTurnEvidenceSql } from './turn-evidence-projection-source';
import { assistantQuotaSql } from './assistant-quota-sql';
import {agentFinanceDatedUsage} from '@erpx/server-finance/public/agent-ai-usage-projection';
import {agentSocialMonthlyCount} from '@erpx/server-collaboration/public/agent-ai-usage-projection';
import {agentExpenseMonthlyCount} from '@erpx/server-people_payroll/public/agent-ai-usage-projection';
import {agentRequestMonthlyCount} from '@erpx/server-records/public/agent-ai-usage-projection';
import {agentExecutiveDatedUsage} from '@erpx/server-runtime/public/agent-ai-usage-projection';
import {financeAiCompanyAllowanceSql} from '@erpx/server-manage_erpx/public/finance-ai-quota-sources';
import {clearAgentBootstrapAssertion} from "@erpx/server-workflow/public/agent-bootstrap-assertions";
import {aiRequestQuotaLock} from "@erpx/server-agent/adapters/ai-quota";
import "server-only";

import {type assistantAskSchema} from "@erpx/feature-work_projection/lib/ai/workspace-assistant";
import type {z} from "zod";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {BetaEnvironment} from "@erpx/server-manage_erpx/public/beta-config";
import {ApiError} from "@erpx/server-runtime/public/http";
import {sha256} from "@erpx/server-runtime/public/crypto";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand} from '@erpx/server-workflow/public/commands';
import {workContextGuards} from '@erpx/server-workflow/public/command-guards';
import {iamMutationGuards} from "@erpx/server-identity_access/public/iam-store";
import {aiProviderOverview,aiSnapshotGuard,aiUsageStatement,resolveAiProvider} from "@erpx/server-agent/adapters/ai-runtime";
import {generateProviderJson,withAiGatewayScope} from "@erpx/server-agent/adapters/ai-transport";
import {employeeRequestDetail} from '@erpx/server-records/public/employee-requests-read';
import {getExpense} from '@erpx/server-people_payroll/public/expense-read';
import {getTask} from '@erpx/server-operations/public/staff-task-read';
import {nativeHandbookReads} from "@erpx/feature-collaboration/lib/native-handbook-page";
import {runtimeEnv} from "../../../../packages/server-runtime/src/public/runtime";
import {assistantGovernanceContext,assistantGovernanceGuards,validateAssistantGovernance,recordAssistantInvocation} from "@erpx/server-agent/adapters/agent-governance";
import {pmsWorkspace} from "../../../../packages/server-pms/src/public/pms";
import {posWorkspace} from "@erpx/server-pos/public/pos";
import {commercePage} from '@erpx/server-commerce/public/commerce';
import {financeOverviewCounts} from '@erpx/server-finance/public/finance';
import {closeWorkspace} from "@erpx/server-finance/public/finance-workspace";
import { assertProductionModuleEnabled } from "@erpx/server-pms/public/feature-availability";
import {getWorkPage} from "@erpx/server-work_projection/public/work-summary";
import {homePosts,postView} from '@erpx/server-collaboration/public/social-home';
import {getLeave} from '@erpx/server-people_payroll/public/leave-read';

import type {SqlStatement} from '@erpx/server-runtime/public/runtime/types';
import {BusinessRuleError} from '@erpx/server-shared/index';
import {createWorkspaceAssistantApplication,type AssistantRepository} from '@erpx/server-agent/application/assistant';
import {type Row} from '@erpx/server-agent/domain/assistant';
import type {AiEndpoint} from '@erpx/server-agent/adapters/ai-transport';
const agentWorkspaceTurnByIdSql = (cols:string) => `SELECT ${cols} FROM ai_workspace_turns WHERE id=?`;
async function exposed<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
async function inward<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof ApiError)throw new BusinessRuleError(error.status,error.code,error.message);throw error;}}
function bindRepository(db:SqlDatabase):AssistantRepository<SqlStatement>{return {clearAuthorityAssertions(key:string,companyId:string){return clearAgentBootstrapAssertion(db,key,companyId);},
async conversationTurns(companyId:string,userId:string,assignmentId:string,value:string,key:string,sessionId:string){return (await db.prepare(`${agentTurnEvidenceSql("t.id,t.question,t.paths_json,t.sources_json,t.answer_json,t.english_answer_json,t.state,t.created_at,e.governance_snapshot_json")} WHERE t.company_id=? AND t.user_id=? AND t.assignment_id=? AND t.scope_key=? AND t.conversation_key=? AND t.private_session_id=? AND t.state='ready' ORDER BY t.created_at DESC LIMIT 20`)
    .bind(companyId,userId,assignmentId,value,key,sessionId).all<Row>()).results;},
async previousTurn(id:string){return db.prepare(agentWorkspaceTurnByIdSql("*")).bind(id).first<Row&{user_id:string;assignment_id:string;scope_key:string;company_id:string}>();},
guardRate(id:string,companyId:string,value:number){return assertion(db,id,"assistant-rate",agentAssistantRateGuardSql,[companyId,value]);},
guardQuota(id:string,companyId:string,value:number,value2:string){return assertion(db,id,"assistant-quota",`${assistantQuotaSql()}`,[companyId,value,companyId,value2,companyId,value2,companyId,value2,companyId]);},
async prepareTurn(id:string,companyId:string,userId:string,assignmentId:string,value:string,conversationKey:string,question:string,value2:string,now:number,sessionId:string){return nativeOwnerWrite('ag_workspace_turn_insert',[id,companyId,userId,assignmentId,value,conversationKey,question,value2,now,now,sessionId],`ag-turn:${id}`,{companyId,now},()=>db.prepare(nativeOwnerWriteNoopSql));},
clearReservationAssertions(id:string,companyId:string){return clearAgentBootstrapAssertion(db,id,companyId);},
async prepareAnswer(value:string,sourcesJson:string,now:number,id:string){return nativeOwnerWrite('ag_workspace_turn_ready',[value,sourcesJson,now,id],`ag-turn-ready:${id}`,{now},()=>db.prepare(nativeOwnerWriteNoopSql));},
async prepareFailedTurn(value:string,now:number,id:string){return nativeOwnerWrite('ag_workspace_turn_failed',[value,now,id],`ag-turn-failed:${id}`,{now},()=>db.prepare(nativeOwnerWriteNoopSql));},
async translationTurn(id:string){return db.prepare(agentWorkspaceTurnByIdSql("state,answer_json")).bind(id).first<Row>();},
guardTranslationQuota(id:string,companyId:string,value:number,month:string){return assertion(db,id,"translation-quota",`(SELECT count(*) FROM (SELECT company_id,created_at FROM ai_workspace_turns UNION ALL ${agentExecutiveDatedUsage()} UNION ALL ${agentFinanceDatedUsage()}) calls WHERE company_id=? AND created_at>=?)+(${agentSocialMonthlyCount()})+(${agentExpenseMonthlyCount()})+(${agentRequestMonthlyCount()})<${financeAiCompanyAllowanceSql}`,[companyId,value,companyId,month,companyId,month,companyId,month,companyId]);},
async prepareTranslation(id:string,companyId:string,userId:string,assignmentId:string,value:string,value2:string,value3:"Translate the previous AI response for English display.",value4:string,now:number,sessionId:string){return nativeOwnerWrite('ag_workspace_turn_insert',[id,companyId,userId,assignmentId,value,value2,value3,value4,now,now,sessionId],`ag-turn:${id}`,{companyId,now},()=>db.prepare(nativeOwnerWriteNoopSql));},
clearTranslationAssertions(id:string,companyId:string){return clearAgentBootstrapAssertion(db,id,companyId);},
async prepareEnglishDisplay(value:string,id:string,companyId:string,userId:string){return nativeOwnerWrite('ag_workspace_turn_english',[value,id,companyId,userId],`ag-turn-english:${id}`,{companyId},()=>db.prepare(nativeOwnerWriteNoopSql));},
async prepareTranslationAnswer(value:string,now:number,id:string){return nativeOwnerWrite('ag_workspace_turn_translation_ready',[value,now,id],`ag-turn-translation:${id}`,{now},()=>db.prepare(nativeOwnerWriteNoopSql));},
async prepareFailedTranslation(now:number,id:string){return nativeOwnerWrite('ag_workspace_turn_translation_failed',[now,id],`ag-turn-translation-failed:${id}`,{now},()=>db.prepare(nativeOwnerWriteNoopSql));},async verifyAuthority(actor,key){await db.batch([...workContextGuards(db,actor,key),...iamMutationGuards(db,actor,key),await clearAgentBootstrapAssertion(db,key,actor.companyId)]);},commit:(...args)=>executeCommand(db,...args),async batch(effects){await db.batch(effects);},quotaLock:companyId=>aiRequestQuotaLock(db,companyId),snapshotGuard:(key,snapshot)=>aiSnapshotGuard(db,key,snapshot),usage:async(...args)=>aiUsageStatement(db,...args),governanceGuards:(...args)=>assistantGovernanceGuards(db,...args)};}
function application(db:SqlDatabase,env?:BetaEnvironment,generate=generateProviderJson){return createWorkspaceAssistantApplication({repository:bindRepository(db),sources:{employeeRequestDetail:(...args)=>inward(()=>employeeRequestDetail(db,...args)),getExpense:(...args)=>inward(()=>getExpense(...args)),getTask:(...args)=>inward(()=>getTask(db,...args)),handbookArticle:async(actor,id)=>inward(async()=>(await nativeHandbookReads(actor,actor.holdingSlug)).handbookArticle(actor,id)),pmsWorkspace:(...args)=>inward(()=>pmsWorkspace(...args)),posWorkspace:(...args)=>inward(()=>posWorkspace(...args)),commercePage:(...args)=>inward(()=>commercePage(...args)),financeOverviewCounts:(...args)=>inward(()=>financeOverviewCounts(...args)),closeWorkspace:(...args)=>inward(()=>closeWorkspace(...args)),assertProductionModuleEnabled:(...args)=>inward(()=>assertProductionModuleEnabled(db,...args)),getWorkPage:(...args)=>inward(()=>getWorkPage(...args)),homePosts:(...args)=>inward(()=>homePosts(db,...args)),postView:(...args)=>inward(()=>postView(db,...args)),getLeave:(...args)=>inward(()=>getLeave(...args))},clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},hash:sha256,production:process.env.NODE_ENV==='production',provider:actor=>inward(()=>resolveAiProvider(db,actor.companyId,env!,"text",actor.holdingSlug)),providerOverview:actor=>inward(async()=>aiProviderOverview(db,actor,await runtimeEnv())),bindGateway:(endpoint,...args)=>withAiGatewayScope(endpoint as AiEndpoint,...args),generate:(endpoint,...args)=>inward(()=>generate(endpoint as AiEndpoint,...args)),generateTranslation:(endpoint,...args)=>inward(()=>generateProviderJson(endpoint as AiEndpoint,...args)),governance:{context:(...args)=>inward(()=>assistantGovernanceContext(db,...args)),validate:(...args)=>inward(()=>validateAssistantGovernance(db,...args)),record:(...args)=>inward(()=>recordAssistantInvocation(db,...args))}});}
export async function assistantSources(db:SqlDatabase,actor:WorkContext,path:string){return exposed(()=>application(db).assistantSources(actor,path));}
export async function assistantView(db:SqlDatabase,actor:WorkContext,env:BetaEnvironment,path:string,mode:"page"|"workspace"){return exposed(()=>application(db,env).assistantView(actor,path,mode));}
export async function askAssistant(db:SqlDatabase,actor:WorkContext,env:BetaEnvironment,input:z.infer<typeof assistantAskSchema>,id:string,generate=generateProviderJson){return exposed(()=>application(db,env,generate).askAssistant(actor,input,id));}
