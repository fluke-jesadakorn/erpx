import "server-only";
import type { WorkContext } from "./authorization";
import { holdingResources } from "./runtime";
import { companyAuthority } from "./staff-policy";
import { canManagePeople } from "./people-directory";
import type { AdminAccess, AdminSection } from "@/lib/administration";

export async function administrationAccess(
  actor: WorkContext,
): Promise<AdminAccess> {
  const { write } = await holdingResources(actor.holdingId);
  const [owner, companyManager, manageAiProviders] = await Promise.all([
    companyAuthority(write, actor, ["holding_owner"]),
    companyAuthority(write, actor),
    companyAuthority(write, actor, ["holding_owner", "it_admin"]),
  ]);
  const managePeople = actor.beta ? owner : canManagePeople(actor);
  const manageOrganization = companyManager;
  const sections: AdminSection[] = [];
  if (managePeople) sections.push("people");
  if (manageOrganization || (actor.beta && owner))
    sections.push("organization");
  // Published routines were already readable by every ERP workspace member.
  // Access to this section does not grant access to the approval queue.
  if (!actor.beta || companyManager) sections.push("approvals");
  if (owner) sections.push("usage");
  if (manageAiProviders) {
    if (owner) sections.push("ai");
    else sections.unshift("ai");
  }
  sections.push("feedback");
  return {
    sections,
    managePeople,
    manageOrganization,
    owner,
    manageAiProviders,
  };
}
