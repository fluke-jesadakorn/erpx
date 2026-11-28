import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import { administrationAccess } from "@erpx/platform-runtime-server/public/administration";
import {appointmentOverview} from '@erpx/organization-server/public/department-authority-reads';
import { departmentFlowSettingsHistory } from "@erpx/organization-server/public/flow-settings-facts";
import { draftCompanyName } from "@erpx/organization-server/public/people-app-entrypoint-facts";
import { companyAuthority } from "@erpx/operations-server/public/staff-policy";
import { serviceTeamOptions } from "@erpx/support-server/public/service-team-options";
import { serviceCoverageSettingsHistory } from "@erpx/support-server/public/flow-settings-facts";
import { leavePolicySettingsHistory } from "@erpx/people-payroll-server/public/flow-settings-facts";
import { nativeActiveLeavePolicySettings } from "@erpx/people-payroll-server/adapters/native-leave-policies";
import { spendingSettingsHistory } from "@erpx/workflow-server/public/flow-settings-facts";
import { createFlowSettingsHubQuery } from "@erpx/work-projection-server/application/flow-settings-hub";

export function flowSettingsHub(actor: WorkContext, tab: string) {
  let db: SqlDatabase;
  return createFlowSettingsHubQuery({
    prepare: async () => { db = (await holdingResources(actor.holdingId)).write; },
    access: () => administrationAccess(actor),
    authority: () => appointmentOverview(db, actor),
    departmentHistory: ids => departmentFlowSettingsHistory(db, actor.companyId, ids),
    canManageLeave: () => companyAuthority(db, actor, "leave.policy.manage"),
    serviceOptions: () => serviceTeamOptions(db, actor),
    leavePolicies: () => nativeActiveLeavePolicySettings(actor),
    leaveHistory: () => leavePolicySettingsHistory(db, actor.companyId),
    serviceHistory: () => serviceCoverageSettingsHistory(db, actor.companyId),
    spendingHistory: () => spendingSettingsHistory(db, actor.companyId),
    companyName: async () => (await draftCompanyName(db, actor.companyId, actor.companyId))?.display_name ?? null,
  })(tab);
}
