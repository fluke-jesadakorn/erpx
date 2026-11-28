import "server-only";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import {overtimeSetup} from '@erpx/people-payroll-server/public/overtime';
import { activeStaff } from "@erpx/operations-server/public/staff-policy";
import { overtimeRosterShift } from "@erpx/people-payroll-server/public/people-app-entrypoint-facts";
import { staffSettings } from "@erpx/operations-server/public/staff-settings";
import {organizationOverview} from '@erpx/organization-server/public/organization';
import { peopleLifecycleData } from "@erpx/people-payroll-server/public/people-lifecycle";
import { personLevelHistory } from "@erpx/organization-server/public/people-app-entrypoint-facts";
import { companyAuthority } from "@erpx/operations-server/public/staff-policy";
import {clearanceCase} from '@erpx/people-payroll-server/public/retirement-clearance';
import {clearanceOwners} from '@erpx/people-payroll-server/public/retirement-clearance';
import { hireHandoffPayGroups } from "@erpx/people-payroll-server/public/people-app-entrypoint-facts";
import { hireHandoffPeople } from "@erpx/people-payroll-server/public/people-app-entrypoint-facts";
import { leaveDefaultLevels } from "@erpx/organization-server/public/people-app-entrypoint-facts";
import { defaultCatalog } from "@erpx/organization-server/public/staff-defaults";

type ReadArguments<F extends (...args: never[]) => unknown> = Parameters<F> extends [unknown, ...infer Rest] ? Rest : never;

/** Native composition for presentation readers; the supplied Holding connection stays private. */
export async function peoplePayrollPresentationQueries(holdingId: string) {
  const { write } = await holdingResources(holdingId);
  return {
    overtimeSetup: (...args: ReadArguments<typeof overtimeSetup>) => overtimeSetup(write, ...args),
    activeStaff: (...args: ReadArguments<typeof activeStaff>) => activeStaff(write, ...args),
    overtimeRosterShift: (...args: ReadArguments<typeof overtimeRosterShift>) => overtimeRosterShift(write, ...args),
    staffSettings: (...args: ReadArguments<typeof staffSettings>) => staffSettings(write, ...args),
    organizationOverview: (...args: ReadArguments<typeof organizationOverview>) => organizationOverview(write, ...args),
    peopleLifecycleData: (...args: ReadArguments<typeof peopleLifecycleData>) => peopleLifecycleData(write, ...args),
    personLevelHistory: (...args: ReadArguments<typeof personLevelHistory>) => personLevelHistory(write, ...args),
    companyAuthority: (...args: ReadArguments<typeof companyAuthority>) => companyAuthority(write, ...args),
    clearanceCase: (...args: ReadArguments<typeof clearanceCase>) => clearanceCase(write, ...args),
    clearanceOwners: (...args: ReadArguments<typeof clearanceOwners>) => clearanceOwners(write, ...args),
    hireHandoffPayGroups: (...args: ReadArguments<typeof hireHandoffPayGroups>) => hireHandoffPayGroups(write, ...args),
    hireHandoffPeople: (...args: ReadArguments<typeof hireHandoffPeople>) => hireHandoffPeople(write, ...args),
    leaveDefaultLevels: (...args: ReadArguments<typeof leaveDefaultLevels>) => leaveDefaultLevels(write, ...args),
    defaultCatalog: (...args: ReadArguments<typeof defaultCatalog>) => defaultCatalog(write, ...args),
  };
}
