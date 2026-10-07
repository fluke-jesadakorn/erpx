import "server-only";
import { runtimeEnv, holdingResources } from "../../runtime/public/runtime";
import type { WorkContext } from "../../identity_access/public/authorization";
import { companyDirectoryMemberIds, directoryAccounts } from "../../identity_access/public/member-directory";
import { canManagePeople, directoryPerson } from "../../people_payroll/public/people-directory";
import { organizationPersonLinks } from "../../people_payroll/public/organization-person-links";
import { createMemberDirectoryApplication } from "../application/member-directory";

export async function companyMembers(actor: WorkContext) {
  const { write } = await holdingResources(actor.holdingId), env = await runtimeEnv();
  return createMemberDirectoryApplication({ userId: actor.userId, companyId: actor.companyId, branchId: actor.branchId, manager: canManagePeople(actor), beta: Boolean(actor.beta) }, {
    memberIds: (scope, now) => companyDirectoryMemberIds(write, { companyId: scope.companyId, branchId: scope.branchId, includeInactive: scope.manager }, now),
    accounts: ids => directoryAccounts(env.CONTROL_DB, ids),
    personLinks: (companyId, accounts, allowBetaEmailFallback) => organizationPersonLinks(write, companyId, accounts, allowBetaEmailFallback),
  }, { now: () => Date.now() }).read();
}

export async function memberProfile(actor: WorkContext, personId: string | null) { return personId ? directoryPerson(actor, personId) : null; }
