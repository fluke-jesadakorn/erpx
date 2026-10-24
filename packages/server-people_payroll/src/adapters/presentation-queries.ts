import "server-only";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import {overtimeSetup} from '@erpx/server-people_payroll/public/overtime';
import { activeStaff } from "@erpx/server-operations/public/staff-policy";
import { overtimeRosterShift } from "@erpx/server-people_payroll/public/people-app-entrypoint-facts";
import { staffSettings } from "@erpx/server-operations/public/staff-settings";
import {organizationOverview} from '@erpx/server-organization/public/organization';
import { peopleLifecycleData } from "@erpx/server-people_payroll/public/people-lifecycle";
import { personLevelHistory } from "@erpx/server-organization/public/people-app-entrypoint-facts";
import { companyAuthority } from "@erpx/server-operations/public/staff-policy";
import {clearanceCase} from '@erpx/server-people_payroll/public/retirement-clearance';
import {clearanceOwners} from '@erpx/server-people_payroll/public/retirement-clearance';
import { hireHandoffPayGroups } from "@erpx/server-people_payroll/public/people-app-entrypoint-facts";
import { hireHandoffPeople } from "@erpx/server-people_payroll/public/people-app-entrypoint-facts";
import { leaveDefaultLevels } from "@erpx/server-organization/public/people-app-entrypoint-facts";
import { defaultCatalog } from "@erpx/server-organization/public/staff-defaults";

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
