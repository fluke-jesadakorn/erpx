import "server-only";
import { hasPermission } from "@/lib/identity_access/permissions";
import { internalAccessPeople } from "../../people_payroll/public/iam-subject-facts";
import { runtimeEnv, holdingResources } from "../../runtime/public/runtime";
import type { WorkContext } from "../../identity_access/public/authorization";
import { companyDirectoryMemberIds, directoryAccounts } from "../../identity_access/public/member-directory";
import { canManagePeople, directoryPerson } from "../../people_payroll/public/people-directory";
import { organizationPersonLinks } from "../../people_payroll/public/organization-person-links";
import { createMemberDirectoryApplication } from "../application/member-directory";

export async function companyMembers(actor: WorkContext) {
  const { write } = await holdingResources(actor.holdingId), env = await runtimeEnv();
  const members = await createMemberDirectoryApplication({ userId: actor.userId, companyId: actor.companyId, branchId: actor.branchId, manager: canManagePeople(actor), beta: Boolean(actor.beta) }, {
    memberIds: (scope, now) => companyDirectoryMemberIds(write, { companyId: scope.companyId, branchId: scope.branchId, includeInactive: scope.manager }, now),
    accounts: ids => directoryAccounts(env.CONTROL_DB, ids),
    personLinks: (companyId, accounts, allowBetaEmailFallback) => organizationPersonLinks(write, companyId, accounts, allowBetaEmailFallback),
  }, { now: () => Date.now() }).read();
  if (!actor.beta && (hasPermission(actor,"people.read") || hasPermission(actor,"people.manage")) && !hasPermission(actor,"iam.read")) {
    const employees = new Set((await internalAccessPeople(write,actor.companyId)).map(p=>p.personId));
    return members.filter(m=>m.personId && employees.has(m.personId));
  }
  return members;
}

export async function memberProfile(actor: WorkContext, personId: string | null) { return personId ? directoryPerson(actor, personId) : null; }
