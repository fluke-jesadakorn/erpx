import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {TalentHireProductEntryScope,TalentProductSourceTarget,TalentSourcePurchaseControls} from "@erpx/server-talent/domain/product-entry";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {readPurchasedAccesses,checkPurchasedCapability} from "@erpx/server-commercial/public/purchase-command-boundary";
import {packageClockSql,purchasedCapabilityPredicate} from "@erpx/server-commercial/public/purchased-capability-sql";
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {ApiError} from "@erpx/server-runtime/public/http";
/** Original hire consumption uses Company people.manage; it never requires a
 * Talent module assignment. This named discovery does not choose an HR role. */
export async function talentHireProductSourceEntry(database:SqlDatabase,scope:TalentHireProductEntryScope,id:string){
 if(scope.branchId||scope.departmentId||!Number.isSafeInteger(scope.assignmentVersion)||scope.assignmentVersion<1)return null;
 const source=await database.prepare(`WITH hire_entry_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms)
 SELECT h.id FROM talent_hire_handoffs h JOIN talent_offers o ON o.id=h.offer_id AND o.company_id=h.company_id
 JOIN talent_applications a ON a.id=o.application_id AND a.company_id=h.company_id
 JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=h.company_id AND r.branch_id=h.branch_id AND r.department_id IS NOT DISTINCT FROM h.department_id
 JOIN talent_candidates c ON c.id=h.candidate_id AND c.id=a.candidate_id AND c.company_id=h.company_id
 CROSS JOIN (SELECT ?::text company_id,NULL::text branch_id,NULL::text department_id) hire_entry CROSS JOIN hire_entry_clock
 WHERE h.id=? AND h.company_id=hire_entry.company_id AND o.status='accepted' AND h.source_offer_version<=o.version
 AND ${assignmentReadSql("hire_entry","people.manage")} AND EXISTS(SELECT 1 FROM assignments WHERE id=? AND version=?)
 AND ${scheduledFeaturePredicate("h.company_id","talent")} AND ${scheduledFeaturePredicate("h.company_id","people_payroll")}
 AND ${purchasedCapabilityPredicate("h.company_id","recruit.offers","history","hire_entry_clock.now_ms")}`)
 .bind(scope.companyId,id,scope.assignmentId,scope.userId,scope.role,scope.assignmentId,scope.assignmentVersion).first();
 return source?{capability:"recruit.offers" as const,purchasePurpose:"history" as const}:null;
}
/** Candidate preservation is an original Company HR/Legal permission. No
 * contact-bearing Talent workspace or general Legal access is granted here. */
export async function talentCandidatePrivacyProductSourceEntry(database:SqlDatabase,scope:TalentHireProductEntryScope,id:string){
 if(scope.branchId||scope.departmentId||!Number.isSafeInteger(scope.assignmentVersion)||scope.assignmentVersion<1)return null;
 const permissions=["legal.hold","legal.hold.release","legal.destroy","people.manage"],row=await database.prepare(`WITH candidate_privacy_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms)
 SELECT c.id FROM talent_candidates c CROSS JOIN (SELECT ?::text company_id,NULL::text branch_id,NULL::text department_id) candidate_privacy_entry CROSS JOIN candidate_privacy_clock
 WHERE c.id=? AND c.company_id=candidate_privacy_entry.company_id AND (${permissions.map(permission=>assignmentReadSql("candidate_privacy_entry",permission)).join(" OR ")}) AND EXISTS(SELECT 1 FROM assignments WHERE id=? AND version=?)
 AND ${scheduledFeaturePredicate("c.company_id","talent")}
 AND ${purchasedCapabilityPredicate("c.company_id","recruit.candidates","history","candidate_privacy_clock.now_ms")}`).bind(scope.companyId,id,...permissions.flatMap(()=>[scope.assignmentId,scope.userId,scope.role]),scope.assignmentId,scope.assignmentVersion).first();
 return row?{capability:"recruit.candidates" as const,purchasePurpose:"history" as const}:null;
}

/** Server-only read gate for the two existing source pages. The closure retains
 * the captured assignment version; only its minimal purchase result is returned
 * by owner workspaces. It grants no mutation, discovery or general People read. */
export async function beginTalentProductSourceRead(actor:WorkContext,target:TalentProductSourceTarget){
 const database=(await holdingResources(actor.holdingId)).write,version=actor.authority?.assignmentVersion??actor.iam?.queuedAssignmentVersion??await database.prepare("SELECT version FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role=?").bind(actor.assignmentId,actor.userId,actor.companyId,actor.role).first<number>("version");
 const scope:TalentHireProductEntryScope={companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId,userId:actor.userId,assignmentId:actor.assignmentId,assignmentVersion:version??0,role:actor.role},capability=target.kind==="hire"?"recruit.offers":"recruit.candidates",permission=target.kind==="hire"?"people.manage":["legal.hold","legal.hold.release","legal.destroy","people.manage"].find(value=>hasPermission(actor,value));
 const verify=async():Promise<TalentSourcePurchaseControls>=>{
  if(!permission)throw new ApiError(403,"CANDIDATE_PRIVACY_FORBIDDEN","A Company HR or Legal privacy permission is required.");
  // Lazy loading retains the existing Talent / People composition module order.
  const {requireTalentCurrentAuthority}=await import("@erpx/server-talent/adapters/purchases");await requireTalentCurrentAuthority(database,actor,[permission],`talent-product-source:${crypto.randomUUID()}`);
  await checkPurchasedCapability(database,"enforced",{companyId:actor.companyId,capability,purpose:"history"});
  const source=target.kind==="hire"?await talentHireProductSourceEntry(database,scope,target.id):await talentCandidatePrivacyProductSourceEntry(database,scope,target.id);
  if(!source)throw new ApiError(404,target.kind==="hire"?"HIRE_HANDOFF_NOT_FOUND":"CANDIDATE_NOT_FOUND","The current purchased source is unavailable in this Company.");
  const access=(await readPurchasedAccesses(database,"enforced",actor.companyId,[capability]))[capability];if(!access.history)await checkPurchasedCapability(database,"enforced",{companyId:actor.companyId,capability,purpose:"history"});return {newWork:access.newWork,settleExisting:access.settleExisting,history:access.history,revision:access.revision};
 };await verify();return verify;
}
