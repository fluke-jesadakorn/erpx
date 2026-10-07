import {peoplePayrollFinalOwnerProjections as operationsFinalOwnerProjections} from "../../operations/public/people-payroll-final-owner-projections";
const operationsFinalSources=operationsFinalOwnerProjections();
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { type TaskDetail } from '../../operations/public/staff-tasks-contract';
export function StaffTasksPeoplePayrollFacts(database: SqlDatabase) {
    return { readTaskDetailDelegationHistory(companyId: string, id: string) {
            return database.prepare(`
    SELECT d.id,COALESCE(recipient.legal_name,d.delegate_user_id) delegate_name,
      COALESCE(grantor.legal_name,d.delegator_user_id) delegator_name,
      d.starts_at,d.expires_at,d.reason,d.revoked_at,d.revoke_reason,
      COALESCE(revoker.legal_name,d.revoked_by) revoked_by_name,
      CASE WHEN d.status='revoked' THEN 'revoked'
        WHEN d.expires_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint THEN 'expired'
        ELSE 'active' END status
    ${operationsFinalSources.operations_staff_tasks_facts_staff_task_delegations_d_from}
    LEFT JOIN hr_people recipient ON recipient.company_id=d.company_id AND recipient.user_id=d.delegate_user_id AND recipient.merged_into_id IS NULL
    LEFT JOIN hr_people grantor ON grantor.company_id=d.company_id AND grantor.user_id=d.delegator_user_id AND grantor.merged_into_id IS NULL
    LEFT JOIN hr_people revoker ON revoker.company_id=d.company_id AND revoker.user_id=d.revoked_by AND revoker.merged_into_id IS NULL
    WHERE d.company_id=? AND d.task_id=? ORDER BY d.created_at DESC,d.id DESC LIMIT 50
  `).bind(companyId, id).all<TaskDetail["delegationHistory"][number]>();
        } };
}
