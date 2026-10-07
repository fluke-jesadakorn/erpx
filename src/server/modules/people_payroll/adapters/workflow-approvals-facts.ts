import {peoplePayrollFinalOwnerProjections as organizationFinalOwnerProjections} from "../../organization/public/people-payroll-final-owner-projections";
const organizationFinalSources=organizationFinalOwnerProjections();
import {peoplePayrollFinalOwnerProjections as workflowFinalOwnerProjections} from "../../workflow/public/people-payroll-final-owner-projections";
const workflowFinalSources=workflowFinalOwnerProjections();
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {assertion} from "@/server/modules/workflow/public/commands";
export function createApprovalsSourceRepository(database:SqlDatabase){return {guard4(key:string,value:"override-current-chain",companyId:string,assignedUserId:string,companyId2:string,companyId3:string,userId:string){return assertion(database,key,value,`EXISTS(WITH RECURSIVE chain AS (
      SELECT e.manager_person_id,l.level_number selected_level,ARRAY[e.person_id] seen,0 depth
      FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id ${organizationFinalSources.workflow_approvals_facts_staff_levels_l_join} ON l.id=e.level_id AND l.company_id=e.company_id
      WHERE e.company_id=? AND e.status='active' AND iam_same_person(p.user_id,?,e.company_id)
      UNION ALL SELECT e.manager_person_id,c.selected_level,c.seen||e.person_id,c.depth+1
      FROM chain c JOIN hr_engagements e ON e.person_id=c.manager_person_id AND e.company_id=? AND e.status='active'
      WHERE NOT e.person_id=ANY(c.seen) AND c.depth<100)
      SELECT 1 FROM chain c JOIN hr_people p ON p.id=c.manager_person_id AND p.company_id=? JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id AND e.status='active' ${organizationFinalSources.workflow_approvals_facts_staff_levels_l_join} ON l.id=e.level_id AND l.company_id=e.company_id
      WHERE iam_same_person(p.user_id,?,p.company_id) AND l.level_number<c.selected_level)`,
      [companyId,assignedUserId,companyId2,companyId3,userId]);},
readInitializeLegacyApprovalExpense(record_id:string,company_id:string){return database.prepare(
            "SELECT created_by,prepared_by FROM finance_expenses WHERE id=? AND company_id=?",
          )
          .bind(record_id, company_id)
          .first<{ created_by: string; prepared_by: string | null }>();},
guard35(key:string,value:"override-reporting-chain",companyId:string,approver_user_id:string,companyId2:string,companyId3:string,userId:string){return assertion(database,key,value,`EXISTS(WITH RECURSIVE chain AS (
      SELECT e.manager_person_id,l.level_number selected_level,ARRAY[e.person_id] seen,0 depth
      FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id ${organizationFinalSources.workflow_approvals_facts_staff_levels_l_join} ON l.id=e.level_id AND l.company_id=e.company_id
      WHERE e.company_id=? AND e.status='active' AND iam_same_person(p.user_id,?,e.company_id)
      UNION ALL
      SELECT e.manager_person_id,c.selected_level,c.seen||e.person_id,c.depth+1
      FROM chain c JOIN hr_engagements e ON e.person_id=c.manager_person_id AND e.company_id=? AND e.status='active'
      WHERE NOT e.person_id=ANY(c.seen) AND c.depth<100)
      SELECT 1 FROM chain c JOIN hr_people p ON p.id=c.manager_person_id AND p.company_id=? JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id AND e.status='active' ${organizationFinalSources.workflow_approvals_facts_staff_levels_l_join} ON l.id=e.level_id AND l.company_id=e.company_id
      WHERE iam_same_person(p.user_id,?,p.company_id) AND l.level_number<c.selected_level)`,[companyId,approver_user_id,companyId2,companyId3,userId]);},
guard37(key:string,value:"override-expense-preparer",id:string,company_id:string,userId:string,userId2:string){return assertion(database,key,value,"EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND (prepared_by IS NULL OR NOT iam_same_person(prepared_by,?,company_id)) AND NOT iam_same_person(created_by,?,company_id))",[id,company_id,userId,userId2]);},
guard38(key:string,value:"expense-separation",id:string,userId:string,userId2:string,id2:string,ordinal:number,userId3:string){return assertion(database,
            key,
            value,
            `EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND NOT iam_same_person(claimant_user_id,?,company_id) AND NOT iam_same_person(prepared_by,?,company_id)) AND NOT EXISTS(SELECT 1 ${workflowFinalSources.workflow_approvals_facts_approval_steps_from} WHERE instance_id=? AND ordinal<? AND state='approved' AND iam_same_person(approver_user_id,?,(SELECT company_id ${workflowFinalSources.workflow_approvals_facts_approval_instances_from} WHERE id=instance_id)))`,
            [
              id,
              userId,
              userId2,
              id2,
              ordinal,
              userId3,
            ],
          );},
readRerouteApprovalEffect(record_id:string,approverUserId:string,approverUserId2:string){return database.prepare(
        "SELECT 1 FROM finance_expenses WHERE id=? AND (created_by=? OR prepared_by=?)",
      )
      .bind(record_id, approverUserId, approverUserId2)
      .first();},
readRerouteApprovalClaim(record_id:string,company_id:string){return database.prepare(
        "SELECT claimant_user_id,prepared_by FROM finance_expenses WHERE id=? AND company_id=?",
      )
      .bind(record_id, company_id)
      .first<{ claimant_user_id: string; prepared_by: string }>();}};}
