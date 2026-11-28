import {peoplePayrollFinalOwnerProjections as organizationFinalOwnerProjections} from "@erpx/organization-server/public/people-payroll-final-owner-projections";
const organizationFinalSources=organizationFinalOwnerProjections();
import {peoplePayrollFinalOwnerProjections as workflowFinalOwnerProjections} from "@erpx/workflow-server/public/people-payroll-final-owner-projections";
const workflowFinalSources=workflowFinalOwnerProjections();
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {ppRd28ApprovalChainSql} from './pp-read-tail-r28-sources';
import {ppRd32TwoColProjection} from './pp-read-tail-r32-sources';
export function createApprovalsSourceRepository(database:SqlDatabase){return {guard4(key:string,value:"override-current-chain",companyId:string,assignedUserId:string,companyId2:string,companyId3:string,userId:string){return assertion(database,key,value,ppRd28ApprovalChainSql(organizationFinalSources.workflow_approvals_facts_staff_levels_l_join),
      [companyId,assignedUserId,companyId2,companyId3,userId]);},
readInitializeLegacyApprovalExpense(record_id:string,company_id:string){return database.prepare(
            ppRd32TwoColProjection('created_by,prepared_by', 'finance_expenses', 'id', 'company_id'),
          )
          .bind(record_id, company_id)
          .first<{ created_by: string; prepared_by: string | null }>();},
guard35(key:string,value:"override-reporting-chain",companyId:string,approver_user_id:string,companyId2:string,companyId3:string,userId:string){return assertion(database,key,value,ppRd28ApprovalChainSql(organizationFinalSources.workflow_approvals_facts_staff_levels_l_join),[companyId,approver_user_id,companyId2,companyId3,userId]);},
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
        ppRd32TwoColProjection('claimant_user_id,prepared_by', 'finance_expenses', 'id', 'company_id'),
      )
      .bind(record_id, company_id)
      .first<{ claimant_user_id: string; prepared_by: string }>();}};}
