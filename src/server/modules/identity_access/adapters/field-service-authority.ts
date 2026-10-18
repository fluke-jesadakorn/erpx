import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import type { ServiceLocationReference, ServiceReferenceScope } from "../../organization/public/field-service-references";
import { finalRootOwnerProjectionSources as people } from "../../people_payroll/public/final-root-owner-projection-sources";

function principalPredicate(alias: "g" | "o" | "r" | "reference", permission: "records.write" | "records.review", principal: "?" | "o.technician_id" | "r.technician_id" | "reference.technician_id") {
  return `EXISTS(SELECT 1 FROM assignments x JOIN (${people.hr_people}) p ON p.user_id=x.user_id AND p.company_id=x.company_id
    JOIN (${people.hr_engagements}) e ON e.person_id=p.id AND e.company_id=p.company_id
    WHERE x.company_id=${alias}.company_id AND x.user_id=${principal}
    AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND iam_principal_active(x.user_id,x.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND iam_role_allows(x.role,x.company_id,'${permission}')
    AND (x.scope_kind='holding' OR x.scope_kind='branch' AND x.scope_node_id=${alias}.branch_id
      OR x.scope_kind='department' AND x.scope_node_id=${alias}.department_id)
    AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active')`;
}
/** Existing Service command policy, shared with its minimal assignment picker. */
export function fieldServiceTechnicianPredicate(alias: "o" | "r" | "reference" = "o") {
  return principalPredicate(alias,"records.write",`${alias}.technician_id`);
}
export function fieldServiceReviewerPredicate(alias: "g" | "o") {
  return principalPredicate(alias,"records.review","?");
}
export type ServiceTechnicianReference = { id: string; name: string; branch_id: string; department_id: string | null };

export async function fieldServiceTechnicians(database: SqlDatabase, scope: ServiceReferenceScope, locations: readonly ServiceLocationReference[]): Promise<ServiceTechnicianReference[]> {
  if (!locations.length) return [];
  const scopes = locations.map(node=>({branch_id:node.branch_id,department_id:node.kind==='department'?node.id:null}))
    .filter(location=>!scope.departmentId || location.department_id===scope.departmentId);
  return (await database.prepare(`SELECT DISTINCT reference.technician_id id,reference.name,reference.branch_id,reference.department_id
    FROM (SELECT p.user_id technician_id,p.legal_name name,p.company_id,scope.branch_id,scope.department_id
      FROM (${people.hr_people}) p CROSS JOIN jsonb_to_recordset(?::jsonb) AS scope(branch_id text,department_id text)
      WHERE p.company_id=? AND p.user_id IS NOT NULL AND p.status='active' AND p.merged_into_id IS NULL) reference
    WHERE ${fieldServiceTechnicianPredicate("reference")} ORDER BY reference.name,reference.technician_id,reference.branch_id,reference.department_id`)
    .bind(JSON.stringify(scopes),scope.companyId).all<ServiceTechnicianReference>()).results;
}
