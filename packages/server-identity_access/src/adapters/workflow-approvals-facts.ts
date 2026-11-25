import {identityVerifiedContextProjections as organizationFinalOwnerProjections} from "@erpx/server-organization/public/identity-access-verified-context-projections";
const organizationFinalSources=organizationFinalOwnerProjections();
import {identitySnapshotSources as peoplepayrollSnapshotSources} from '@erpx/server-people_payroll/public/identity-snapshot-sources';
const peoplepayrollSources=peoplepayrollSnapshotSources();
export const overrideAuthorityProbe=(engagementJoin:string)=>`EXISTS(${assignmentAuthorityProbeSql(`a.revoked_at IS NULL AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (iam_role_allows(a.role,a.company_id,'workflow.people_manager') AND (a.scope_kind='holding' OR a.scope_kind='branch' AND a.scope_node_id=? OR a.scope_kind='department' AND a.scope_node_id=?) OR EXISTS(SELECT 1 ${organizationFinalSources.workflow_approvals_facts_authority_seats_s_from} ${peoplepayrollSources.workflow_approvals_facts_hr_people_p_join} ON p.id=s.person_id AND p.company_id=s.company_id ${engagementJoin}WHERE s.company_id=a.company_id AND s.kind IN ('ceo','hr') AND iam_same_person(p.user_id,a.user_id,a.company_id) AND s.effective_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint))`)})`;
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {assignmentUserScopeProbeSql} from './read-tail-family-r29';
import {assignmentCompanyProbeSql} from './read-tail-family-r30';
import {assignmentAuthorityProbeSql} from './read-tail-family-r33';
import {nativeApprovalActiveScope,nativeApprovalCanOverride,nativeApprovalExpenseEvidence,type LaneHoldingScope} from './native-lane-12/client';
/** The three owner-local assignment reads below are executed by native lane 12;
 * command guards and their cross-owner assertions remain in TypeScript. */
export function createApprovalsSourceRepository(database:SqlDatabase,scope?:LaneHoldingScope){return {readCanOverrideStaffStepRole(assignmentId:string,companyId:string,userId:string,branchId:string | null,departmentId:string | null){return nativeApprovalCanOverride(companyId,assignmentId,userId,branchId,departmentId,scope);},
guard3(key:string,value:"override-current-right",assignmentId:string,companyId:string,userId:string,branchId:string | null,departmentId:string | null){return assertion(database,key,value,overrideAuthorityProbe(`${peoplepayrollSources.workflow_approvals_facts_hr_engagements_e_join} ON e.person_id=p.id AND e.company_id=p.company_id AND e.status='active' `),
      [assignmentId,companyId,userId,branchId,departmentId]);},
guard5(key:string,value:"override-independent",userId:string,requesterUserId:string,companyId:string,preparerUserId:string | null,userId2:string,preparerUserId2:string | null,companyId2:string){return assertion(database,key,value,`NOT iam_same_person(?,?,?) AND (?::text IS NULL OR NOT iam_same_person(?,?,?))`,
      [userId,requesterUserId,companyId,preparerUserId,userId2,preparerUserId2,companyId2]);},
guard26(key:string,label:string,approver_user_id:string,company_id:string,authority:"named" | "department" | "hr_admin" | "holding_owner",now:number,now2:number,authority2:"named" | "department" | "hr_admin" | "holding_owner",branch_id:string | null,department_id:string | null){return assertion(database,
    key,
    label,
    `EXISTS(${assignmentUserScopeProbeSql(`iam_role_allows(role,company_id,'workflow.'||?) AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>? AND
    (scope_kind='holding' OR (?='hr_admin' AND ((scope_kind='branch' AND scope_node_id=?) OR (scope_kind='department' AND scope_node_id=?))))`)})`,
    [
      approver_user_id,
      company_id,
      authority,
      now,
      now2,
      authority2,
      branch_id,
      department_id,
    ],
  );},
readApprovalViewActiveScope(assignmentId:string,userId:string,companyId:string,role:string,now:number,now2:number,policyStep:boolean,branch_id:string | null,department_id:string | null){return nativeApprovalActiveScope(companyId,assignmentId,userId,role,now,now2,policyStep,branch_id,department_id,scope);},
readExpenseDecisionEvidenceAssignmentsreceiptsperson(company_id:string,assignmentId:string,now:number,now2:number){return nativeApprovalExpenseEvidence(company_id,assignmentId,now,now2,scope);},
guard32(key:string,value:"override-authority",assignmentId:string,companyId:string,userId:string,branch_id:string | null,department_id:string | null){return assertion(database,key,value,overrideAuthorityProbe(''),[assignmentId,companyId,userId,branch_id,department_id]);},
guard34(key:string,value:"active-decision-role",assignmentId:string,value2:string){return assertion(database, key, value, "EXISTS(SELECT 1 FROM assignments WHERE id=? AND iam_role_allows(role,company_id,'workflow.'||?))", [assignmentId, value2]);},
guard39(key:string,value:"expense-role",company_id:string,assignmentId:string,required_role:string,now:number,now2:number,branch_id:string | null,department_id:string | null){return assertion(database,
                  key,
                  value,
                  `EXISTS(${assignmentCompanyProbeSql("id=? AND iam_role_allows(role,company_id,'workflow.'||?) AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>? AND (scope_kind='holding' OR (scope_kind='branch' AND scope_node_id=?) OR (scope_kind='department' AND scope_node_id=?))")})`,
                  [
                    company_id,
                    assignmentId,
                    required_role,
                    now,
                    now2,
                    branch_id,
                    department_id,
                  ],
                );}};}
