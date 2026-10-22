import {peoplePayrollFinalOwnerProjections as organizationFinalOwnerProjections} from "@erpx/server-organization/public/people-payroll-final-owner-projections";
const organizationFinalSources=organizationFinalOwnerProjections();
import {peoplePayrollFinalOwnerProjections as identityaccessFinalOwnerProjections} from "@erpx/server-identity_access/public/people-payroll-final-owner-projections";
const identityaccessFinalSources=identityaccessFinalOwnerProjections();
import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export async function memberProfileWork(write:SqlDatabase,userId:string){
    return (await write.prepare(`SELECT c.display_name AS company_name,COALESCE(op.title,e.job_title) AS job_title
      FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id
      ${organizationFinalSources.profile_work_organization_nodes_c_join} ON c.id=e.company_id AND c.kind='company' AND c.status='active'
      LEFT ${organizationFinalSources.profile_work_organization_positions_op_join} ON op.id=e.position_id AND op.active=1
      WHERE (p.user_id=? OR EXISTS(SELECT 1 ${identityaccessFinalSources.profile_work_iam_members_m_from} WHERE m.person_id=p.id AND m.user_id=? AND m.company_id=p.company_id))
        AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active'
      ORDER BY e.start_date DESC LIMIT 3`).bind(userId,userId).all<{company_name:string;job_title:string|null}>()).results;
}
