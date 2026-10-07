import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { organizationRoster } from "@/server/modules/organization/public/organization";
import { reportingIssues, type OrganizationPerson } from "@/lib/organization";

/** Native owner facts retain the exact current placement and reporting policy. */
export async function reportingRosterFacts(db: SqlDatabase, companyId: string) {
  const roster = await organizationRoster(db, companyId);
  return { people: roster.people, issues: (person: OrganizationPerson) => reportingIssues(person, roster.people, roster.positions) };
}
export async function activeDepartmentFact(db: SqlDatabase, companyId: string, departmentId: string) {
  return Boolean(await db.prepare("SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='department' AND status='active'").bind(departmentId, companyId).first());
}
