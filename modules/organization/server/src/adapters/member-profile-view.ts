import "server-only";
import { hasPermission } from "@erpx/identity-access-ui/lib/permissions";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { canManagePeople, directoryOptions, personAccess } from "@erpx/people-payroll-server/public/people-directory";
import { staffSettings } from "@erpx/operations-server/public/staff-settings";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import {organizationOverview} from '@erpx/organization-server/public/organization';
import { memberProfile } from "@erpx/organization-server/public/member-directory";
import { createMemberProfileViewApplication } from "@erpx/organization-server/application/member-profile-view";
import { levelHistorySql } from "./organization-read-tail-sources-r60";
import type { MemberLevelHistory, MemberProfileReference } from "@erpx/organization-server/domain/member-profile-view";

export async function memberProfileLevelHistory(write: SqlDatabase, engagementId: string | number): Promise<MemberLevelHistory[]> {
  return (await write.prepare(
    levelHistorySql(" FROM staff_level_history h JOIN staff_levels l ON l.id=h.level_id WHERE h.engagement_id=? ORDER BY h.created_at DESC"),
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
