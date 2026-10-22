import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
/** Workflow owns retained Finance decision evidence. The caller stages these statements on its original transaction, preserving its final audit/outbox order. */
export function financeOwnerDecisionEffects(db:SqlDatabase){return {
appendRetainedLayoutDecision:(facts:{id:string;commandId:string;recordId:string;action:string;actorId:string;assignmentId:string;reviewedVersion:number;inputDigest:string;createdAt:number})=>db.prepare("INSERT INTO decision_evidence VALUES(?,?,?,?,?,?,?,?,?)").bind(facts.id,facts.commandId,facts.recordId,facts.action,facts.actorId,facts.assignmentId,facts.reviewedVersion,facts.inputDigest,facts.createdAt),
appendReviewedDecision:(facts:{id:string;commandId:string;recordId:string;action:string;actorId:string;assignmentId:string;reviewedVersion:number;inputDigest:string;createdAt:number})=>db.prepare("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(facts.id,facts.commandId,facts.recordId,facts.action,facts.actorId,facts.assignmentId,facts.reviewedVersion,facts.inputDigest,facts.createdAt)
};}
/** Shared reviewed-decision layout, prepared on the caller's supplied transaction. */
export const workflowDecisionEffects = financeOwnerDecisionEffects;
