import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {ApprovalStep} from "@erpx/server-workflow/public/approvals";
export function legalApprovalFacts(db:SqlDatabase){return {
routeSteps:(facts:{instanceId:string;})=>db.prepare("SELECT * FROM approval_steps WHERE instance_id=? ORDER BY ordinal").bind(facts.instanceId).all(),
decisionHistory:(facts:{id:string;companyId:string;})=>db.prepare("SELECT d.* FROM approval_decisions d JOIN approval_instances i ON i.id=d.instance_id WHERE i.record_type='legal' AND i.record_id=? AND i.company_id=? ORDER BY d.insertion_order").bind(facts.id,facts.companyId).all(),
currentStep:(facts:{instanceId:string;currentStep:number;})=>db.prepare("SELECT * FROM approval_steps WHERE instance_id=? AND ordinal=?").bind(facts.instanceId,facts.currentStep).first<ApprovalStep>(),
advanceRoute:(facts:{now:number;instanceId:string;})=>db.prepare("UPDATE approval_instances SET version=version+1,updated_at=? WHERE id=?").bind(facts.now,facts.instanceId),
replaceReviewer:(facts:{approverId:string;id:string;})=>db.prepare("UPDATE approval_steps SET approver_user_id=? WHERE id=?").bind(facts.approverId,facts.id),
recordReroute:(facts:{key:string;instanceId:string;id:string;actorId:string;reason:string;version:number;key2:string;now:number;approver_user_id:string;approverId:string;})=>db.prepare("INSERT INTO approval_decisions(id,instance_id,step_id,action,actor_user_id,reason,reviewed_version,command_id,created_at,previous_approver_user_id,approver_user_id) VALUES(?,?,?,'reroute',?,?,?,?,?,?,?)").bind(facts.key,facts.instanceId,facts.id,facts.actorId,facts.reason,facts.version,facts.key2,facts.now,facts.approver_user_id,facts.approverId),
decideStep:(facts:{value0:"approved" | "rejected";id:string;})=>db.prepare("UPDATE approval_steps SET state=? WHERE id=?").bind(facts.value0,facts.id),
recordDecision:(facts:{key:string;instanceId:string;id:string;decision:"approve" | "reject" | "revise";actorId:string;reason:string;version:number;key2:string;now:number;value9:string;})=>db.prepare("INSERT INTO approval_decisions(id,instance_id,step_id,action,actor_user_id,reason,reviewed_version,command_id,created_at,authority_json) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(facts.key,facts.instanceId,facts.id,facts.decision,facts.actorId,facts.reason,facts.version,facts.key2,facts.now,facts.value9),
nextReviewer:(facts:{instanceId:string;nextStep:number;})=>db.prepare("SELECT approver_user_id FROM approval_steps WHERE instance_id=? AND ordinal=?").bind(facts.instanceId,facts.nextStep).first<{ approver_user_id: string }>(),
cancelRoute:(facts:{value0:"rejected" | "cancelled";now:number;instanceId:string;})=>db.prepare("UPDATE approval_instances SET state=?,version=version+1,updated_at=? WHERE id=?").bind(facts.value0,facts.now,facts.instanceId),
advanceStep:(facts:{now:number;instanceId:string;})=>db.prepare("UPDATE approval_instances SET current_step=current_step+1,version=version+1,updated_at=? WHERE id=?").bind(facts.now,facts.instanceId),
approveRoute:(facts:{now:number;instanceId:string;})=>db.prepare("UPDATE approval_instances SET state='approved',version=version+1,updated_at=? WHERE id=?").bind(facts.now,facts.instanceId)
};}
