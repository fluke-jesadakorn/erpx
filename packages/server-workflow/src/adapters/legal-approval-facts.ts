import "server-only";
import {nativeWorkflowWrite} from './native-workflow-write';
import {nativeWorkflowReadAll} from './native-workflow-read';
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {ApprovalStep} from '@erpx/server-workflow/public/approvals';
export function legalApprovalFacts(db:SqlDatabase){return {
routeSteps:(facts:{instanceId:string;})=>db.prepare("SELECT * FROM approval_steps WHERE instance_id=? ORDER BY ordinal").bind(facts.instanceId).all(),
async decisionHistory(facts:{id:string;companyId:string;}){return {results:await nativeWorkflowReadAll<Record<string,unknown>>('wf_native_legal_decision_history',[facts.id,facts.companyId],facts.companyId,db)};},
currentStep:(facts:{instanceId:string;currentStep:number;})=>db.prepare("SELECT * FROM approval_steps WHERE instance_id=? AND ordinal=?").bind(facts.instanceId,facts.currentStep).first<ApprovalStep>(),
advanceRoute:(facts:{now:number;instanceId:string;})=>nativeWorkflowWrite('wf_legal_advanceroute', [facts.now,facts.instanceId]),
replaceReviewer:(facts:{approverId:string;id:string;})=>nativeWorkflowWrite('wf_legal_replacereviewer', [facts.approverId,facts.id]),
recordReroute:(facts:{key:string;instanceId:string;id:string;actorId:string;reason:string;version:number;key2:string;now:number;approver_user_id:string;approverId:string;})=>nativeWorkflowWrite('wf_legal_recordreroute', [facts.key,facts.instanceId,facts.id,facts.actorId,facts.reason,facts.version,facts.key2,facts.now,facts.approver_user_id,facts.approverId]),
decideStep:(facts:{value0:"approved" | "rejected";id:string;})=>nativeWorkflowWrite('wf_legal_decidestep', [facts.value0,facts.id]),
recordDecision:(facts:{key:string;instanceId:string;id:string;decision:"approve" | "reject" | "revise";actorId:string;reason:string;version:number;key2:string;now:number;value9:string;})=>nativeWorkflowWrite('wf_legal_recorddecision', [facts.key,facts.instanceId,facts.id,facts.decision,facts.actorId,facts.reason,facts.version,facts.key2,facts.now,facts.value9]),
nextReviewer:(facts:{instanceId:string;nextStep:number;})=>db.prepare("SELECT approver_user_id FROM approval_steps WHERE instance_id=? AND ordinal=?").bind(facts.instanceId,facts.nextStep).first<{ approver_user_id: string }>(),
cancelRoute:(facts:{value0:"rejected" | "cancelled";now:number;instanceId:string;})=>nativeWorkflowWrite('wf_legal_cancelroute', [facts.value0,facts.now,facts.instanceId]),
advanceStep:(facts:{now:number;instanceId:string;})=>nativeWorkflowWrite('wf_legal_advancestep', [facts.now,facts.instanceId]),
approveRoute:(facts:{now:number;instanceId:string;})=>nativeWorkflowWrite('wf_legal_approveroute', [facts.now,facts.instanceId])
};}
