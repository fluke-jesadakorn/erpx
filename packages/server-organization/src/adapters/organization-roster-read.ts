import 'server-only';
import type{SqlDatabase}from '@erpx/server-runtime/public/runtime/types';
import type{OrganizationPosition}from '@erpx/feature-organization/lib/organization';
import{policyRead}from '@erpx/server-identity_access/public/iam-read-snapshot';
import{organizationRosterPeople}from '@erpx/server-people_payroll/public/organization-roster-facts';
export async function organizationRoster(db: SqlDatabase, companyId: string) {
  return structuredClone(await policyRead(db,JSON.stringify(["organization-roster",companyId]),async () => {
  const [people, positions] = await Promise.all([
organizationRosterPeople(db,companyId),
    db.prepare("SELECT p.*,l.level_number FROM organization_positions p JOIN staff_levels l ON l.id=p.level_id AND l.company_id=p.company_id WHERE p.company_id=? ORDER BY l.level_number,p.title,p.id").bind(companyId).all<OrganizationPosition>(),
  ]);
  return {people:people.results,positions:positions.results};
  }));
}
