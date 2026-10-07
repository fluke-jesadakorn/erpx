import "server-only";
import type { WorkContext } from "../../identity_access/public/authorization";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import { holdingResources } from "../../runtime/public/runtime";
import { administrationAccess } from "../../runtime/public/administration";
import { appointmentOverview } from "../../organization/public/department-authority";
import { departmentFlowSettingsHistory } from "../../organization/public/flow-settings-facts";
import { draftCompanyName } from "../../organization/public/people-app-entrypoint-facts";
import { companyAuthority } from "../../operations/public/staff-policy";
import { serviceTeamOptions } from "../../support/public/service-team-options";
import { serviceCoverageSettingsHistory } from "../../support/public/flow-settings-facts";
import { activeLeavePolicySettings, leavePolicySettingsHistory } from "../../people_payroll/public/flow-settings-facts";
import { spendingSettingsHistory } from "../../workflow/public/flow-settings-facts";
import { createFlowSettingsHubQuery } from "../application/flow-settings-hub";

export function flowSettingsHub(actor: WorkContext, tab: string) {
  let db: SqlDatabase;
  return createFlowSettingsHubQuery({
    prepare: async () => { db = (await holdingResources(actor.holdingId)).write; },
    access: () => administrationAccess(actor),
    authority: () => appointmentOverview(db, actor),
    departmentHistory: ids => departmentFlowSettingsHistory(db, actor.companyId, ids),
    canManageLeave: () => companyAuthority(db, actor, "leave.policy.manage"),
    serviceOptions: () => serviceTeamOptions(db, actor),
    leavePolicies: () => activeLeavePolicySettings(db, actor.companyId),
    leaveHistory: () => leavePolicySettingsHistory(db, actor.companyId),
    serviceHistory: () => serviceCoverageSettingsHistory(db, actor.companyId),
    spendingHistory: () => spendingSettingsHistory(db, actor.companyId),
    companyName: async () => (await draftCompanyName(db, actor.companyId, actor.companyId))?.display_name ?? null,
  })(tab);
}
