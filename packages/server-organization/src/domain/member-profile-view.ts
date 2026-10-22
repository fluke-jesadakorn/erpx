import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { directoryOptions, directoryPerson, personAccess } from "@erpx/server-people_payroll/public/people-directory";
import type { staffSettings } from "@erpx/server-operations/public/staff-settings";
import type { organizationOverview } from "@erpx/server-organization/public/organization";

export type MemberProfileReference = { id: string; name: string; email: string; personId: string | null };
export type MemberProfilePerson = Awaited<ReturnType<typeof directoryPerson>>;
export type MemberProfileSettings = Awaited<ReturnType<typeof staffSettings>>;
export type MemberProfileAccess = Awaited<ReturnType<typeof personAccess>>;
export type MemberProfileNodes = Awaited<ReturnType<typeof directoryOptions>>;
export type MemberProfileOrganization = Awaited<ReturnType<typeof organizationOverview>>;
export type MemberLevelHistory = {
  id: string;
  engagement_id: string;
  previous_level_id: string | null;
  level_id: string;
  actor_id: string;
  reason: string;
  command_id: string;
  created_at: number;
  level_name: string;
};

export function memberProfileAccessControls(actor: Pick<WorkContext, "userId">, memberId: string, permissions: { manage: boolean; revokeCompany: boolean }) {
  return {
    canManageAccess: permissions.manage && memberId !== actor.userId,
    canRevokeCompany: permissions.revokeCompany && memberId !== actor.userId,
  };
}
