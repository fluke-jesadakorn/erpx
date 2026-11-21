import "server-only";
import {nativeWorkflowWrite} from './native-workflow-write';
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
/** Workflow owns retained Finance decision evidence. The caller stages these statements on its original transaction, preserving its final audit/outbox order. */
export function financeOwnerDecisionEffects(db:SqlDatabase){return {
appendRetainedLayoutDecision:(facts:{id:string;commandId:string;recordId:string;action:string;actorId:string;assignmentId:string;reviewedVersion:number;inputDigest:string;createdAt:number})=>nativeWorkflowWrite('wf_native_finance_owner_decision_effects_6', [facts.id,facts.commandId,facts.recordId,facts.action,facts.actorId,facts.assignmentId,facts.reviewedVersion,facts.inputDigest,facts.createdAt]),
appendReviewedDecision:(facts:{id:string;commandId:string;recordId:string;action:string;actorId:string;assignmentId:string;reviewedVersion:number;inputDigest:string;createdAt:number})=>nativeWorkflowWrite('wf_native_finance_owner_decision_effects_7', [facts.id,facts.commandId,facts.recordId,facts.action,facts.actorId,facts.assignmentId,facts.reviewedVersion,facts.inputDigest,facts.createdAt])
};}
/** Shared reviewed-decision layout, prepared on the caller's supplied transaction. */
export const workflowDecisionEffects = financeOwnerDecisionEffects;
