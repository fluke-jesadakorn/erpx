import "server-only";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { canManagePeople, directoryOptions, personAccess } from "@erpx/server-people_payroll/public/people-directory";
import { staffSettings } from "@erpx/server-operations/public/staff-settings";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {organizationOverview} from '@erpx/server-organization/public/organization';
import { memberProfile } from "@erpx/server-organization/public/member-directory";
import { createMemberProfileViewApplication } from "@erpx/server-organization/application/member-profile-view";
import type { MemberLevelHistory, MemberProfileReference } from "@erpx/server-organization/domain/member-profile-view";

export async function memberProfileLevelHistory(write: SqlDatabase, engagementId: string | number): Promise<MemberLevelHistory[]> {
  return (await write.prepare(
    "SELECT h.*,l.name level_name FROM staff_level_history h JOIN staff_levels l ON l.id=h.level_id WHERE h.engagement_id=? ORDER BY h.created_at DESC",
  ).bind(engagementId).all<MemberLevelHistory>()).results;
}

export async function memberProfileDetails(actor: WorkContext, member: MemberProfileReference) {
  let write: SqlDatabase;
  return createMemberProfileViewApplication({
    profile: memberProfile,
    settings: async scope => {
      write = (await holdingResources(scope.holdingId)).write;
      return staffSettings(write, scope);
    },
    canManagePeople,
    access: personAccess,
    levelHistory: engagementId => memberProfileLevelHistory(write, engagementId),
    nodes: directoryOptions,
    organization: scope => organizationOverview(write, scope),
    hasPermission,
  }).read(actor, member);
}
