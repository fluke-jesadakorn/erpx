import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import {organizationRoster} from '@erpx/organization-server/public/organization-roster-read';
import { reportingIssues, type OrganizationPerson } from "@erpx/organization-ui/lib/organization";
import {nativeOrgOne} from './native-organization-owner-write';

/** Native owner facts retain the exact current placement and reporting policy. */
export async function reportingRosterFacts(db: SqlDatabase, companyId: string) {
  const roster = await organizationRoster(db, companyId);
  return { people: roster.people, issues: (person: OrganizationPerson) => reportingIssues(person, roster.people, roster.positions) };
}
export async function activeDepartmentFact(db: SqlDatabase, companyId: string, departmentId: string) {
  return Boolean(await nativeOrgOne(db, companyId, 'profile_active_department', [departmentId, companyId]));
}
