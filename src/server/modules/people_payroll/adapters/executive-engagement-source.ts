import {peoplePayrollFinalOwnerProjections as organizationFinalOwnerProjections} from "../../organization/public/people-payroll-final-owner-projections";
const organizationFinalSources=organizationFinalOwnerProjections();
import type {SqlDatabase} from '../../runtime/public/runtime/types';import type {WorkContext} from '../../identity_access/public/authorization';
export async function executiveEngagementSource(db:SqlDatabase,a:WorkContext) {
  return !!await db.prepare(`SELECT 1 FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id
    ${organizationFinalSources.executive_engagement_source_staff_levels_l_join} ON l.id=e.level_id AND l.company_id=e.company_id
    WHERE p.company_id=? AND p.user_id=? AND p.status='active' AND p.merged_into_id IS NULL
    AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')
    AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))
    AND l.active=1 AND l.level_number IN (1,2) LIMIT 1`).bind(a.companyId,a.userId).first();
}
