import "server-only";
import {peoplePayrollFinalOwnerProjections} from "@erpx/server-identity_access/public/people-payroll-final-owner-projections";
const identitySources=peoplePayrollFinalOwnerProjections();
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export async function internalAccessPeople(database: SqlDatabase, companyId: string) {
  return (await database.prepare(`SELECT DISTINCT ON (p.id) p.id AS "personId",p.legal_name AS name,
    COALESCE(NULLIF(p.work_email,''),p.email,'') AS email,
    COALESCE(p.user_id,(SELECT m.user_id ${identitySources.people_directory_repository_iam_members_m_from} WHERE m.company_id=p.company_id AND m.person_id=p.id AND m.status='active' ORDER BY m.user_id LIMIT 1)) AS "userId",
    e.department_id AS "departmentId",e.status
    FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id
    WHERE p.company_id=? AND p.merged_into_id IS NULL AND p.status!='archived' AND e.worker_type='employee'
      AND e.status IN ('active','onboarding','suspended')
    ORDER BY p.id,e.start_date DESC,e.id`).bind(companyId).all<{personId:string;name:string;email:string;userId:string|null;departmentId:string|null;status:string}>()).results;
}
