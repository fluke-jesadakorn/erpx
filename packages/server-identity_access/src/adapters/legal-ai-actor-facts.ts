import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function legalAiActorFacts(db:SqlDatabase){return {
actorAssignment:(facts:{assignmentId:string;actorId:string;companyId:string;role:string;fact4:number | null;fact5:number | null;branchId:string | null;departmentId:string | null;now:number;now2:number;})=>db.prepare(`SELECT 1 FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role=?
    AND (?::bigint IS NULL OR version=?)
    AND (scope_kind='holding' OR scope_kind='branch' AND scope_node_id=? OR scope_kind='department' AND scope_node_id=?)
    AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?)
    AND iam_role_allows(role,company_id,'legal.ai')`).bind(facts.assignmentId,facts.actorId,facts.companyId,facts.role,facts.fact4,facts.fact5,facts.branchId,facts.departmentId,facts.now,facts.now2).first(),
actorContext:(facts:{sessionId:string;companyId:string;actorId:string;assignmentId:string;revision:number;assignmentVersion:number;now:number;sessionId2:string;companyId2:string;actorId2:string;})=>db.prepare(`SELECT 1 FROM active_work_contexts WHERE session_id=? AND company_id=? AND user_id=?
    AND assignment_id=? AND revision=? AND assignment_version=? AND expires_at>?
    AND NOT EXISTS(SELECT 1 FROM iam_revoked_sessions WHERE session_id=? AND company_id=? AND user_id=?)`).bind(facts.sessionId,facts.companyId,facts.actorId,facts.assignmentId,facts.revision,facts.assignmentVersion,facts.now,facts.sessionId2,facts.companyId2,facts.actorId2).first(),
assignmentLease:(facts:{assignmentId:string;actorId:string;now:number;now2:number;})=>db.prepare("SELECT 1 FROM assignments WHERE id=? AND user_id=? AND revoked_at IS NULL AND valid_until>? AND valid_from<=?").bind(facts.assignmentId,facts.actorId,facts.now,facts.now2).first(),
assignmentAfterGeneration:(facts:{assignmentId:string;actorId:string;companyId:string;role:string;now:number;now2:number;})=>db.prepare("SELECT 1 FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>?").bind(facts.assignmentId,facts.actorId,facts.companyId,facts.role,facts.now,facts.now2).first()
};}
