import type { appointmentOverview } from "../../organization/public/department-authority";
import type { administrationAccess } from "../../runtime/public/administration";
import type { serviceTeamOptions } from "../../support/public/service-team-options";
import type { departmentFlowSettingsHistory } from "../../organization/public/flow-settings-facts";
import type { activeLeavePolicySettings } from "../../people_payroll/public/flow-settings-facts";

export interface FlowSettingsHistoryRow { title: string; version: number; actor: string; at: number; kind: string }
export interface FlowSettingsHubPorts {
  prepare(): Promise<void>;
  access(): ReturnType<typeof administrationAccess>;
  authority(): ReturnType<typeof appointmentOverview>;
  departmentHistory(ids: string[]): ReturnType<typeof departmentFlowSettingsHistory>;
  canManageLeave(): Promise<boolean>;
  serviceOptions(): ReturnType<typeof serviceTeamOptions>;
  leavePolicies(): ReturnType<typeof activeLeavePolicySettings>;
  leaveHistory(): Promise<FlowSettingsHistoryRow[]>;
  serviceHistory(): Promise<FlowSettingsHistoryRow[]>;
  spendingHistory(): Promise<FlowSettingsHistoryRow[]>;
  companyName(): Promise<string | null>;
}
export function createFlowSettingsHubQuery(ports: FlowSettingsHubPorts) {
  return async (tab: string) => {
    await ports.prepare();
    const access = await ports.access(), authority = await ports.authority();
    const policyHistory = authority.departments.length ? await ports.departmentHistory(authority.departments.map(department => department.id)) : [];
    const canManageLeave = await ports.canManageLeave();
    const serviceOptions = access.owner ? await ports.serviceOptions() : null;
    const leavePolicies = canManageLeave ? await ports.leavePolicies() : [];
    const configurationHistory = tab === "history" ? (await Promise.all([
      canManageLeave ? ports.leaveHistory() : Promise.resolve([]),
      access.owner ? ports.serviceHistory() : Promise.resolve([]),
      access.manageOrganization ? ports.spendingHistory() : Promise.resolve([]),
    ])).flat() : [];
    const companyName = await ports.companyName() ?? "Current Company";
    return { access, authority, policyHistory, canManageLeave, serviceOptions, leavePolicies, configurationHistory, companyName, canPublish: authority.canPublishDepartmentIds.length > 0 };
  };
}
