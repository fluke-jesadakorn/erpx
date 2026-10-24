import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { administrationAccess } from "@erpx/server-runtime/public/administration";
import {appointmentOverview} from '@erpx/server-organization/public/department-authority-reads';
import { departmentFlowSettingsHistory } from "@erpx/server-organization/public/flow-settings-facts";
import { draftCompanyName } from "@erpx/server-organization/public/people-app-entrypoint-facts";
import { companyAuthority } from "@erpx/server-operations/public/staff-policy";
import { serviceTeamOptions } from "@erpx/server-support/public/service-team-options";
import { serviceCoverageSettingsHistory } from "@erpx/server-support/public/flow-settings-facts";
import { activeLeavePolicySettings, leavePolicySettingsHistory } from "@erpx/server-people_payroll/public/flow-settings-facts";
import { spendingSettingsHistory } from "@erpx/server-workflow/public/flow-settings-facts";
import { createFlowSettingsHubQuery } from "@erpx/server-work_projection/application/flow-settings-hub";

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
