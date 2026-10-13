import type { ComponentProps } from "react";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { OrganizationSelection } from "@/components/people_payroll/organization-explorer";
import { ApiError } from "@/server/modules/runtime/public/http";
import type { DepartmentAuthorityWorkspace } from "@/components/organization/department-authority-workspace";
import { administrationAccess } from "@/server/modules/runtime/public/administration";
import { listBrandProfiles } from "@/server/modules/brand/public/branding";
import { appointmentOverview } from "@/server/modules/organization/public/department-authority";
import { holdingDirectorySelection } from "@/server/modules/organization/public/holding-people-directory";
import { organizationOverview } from "@/server/modules/organization/public/organization";
import { directoryOptions } from "@/server/modules/people_payroll/adapters/people-directory";
import { peopleLifecycleData } from "@/server/modules/people_payroll/adapters/people-lifecycle";
import { getPerson, payrollOptions } from "@/server/modules/people_payroll/adapters/people-payroll";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { staffSettings } from "@/server/modules/operations/public/staff-settings";
import type { SqlDatabase } from '../../runtime/public/runtime/types';
import { BusinessRuleError } from '../../shared';
import { peopleLevelHistory } from '../../operations/public/people-level-history';
import { createPeopleExplorerApplication } from '../application/people-explorer-detail';
import type { PeopleExplorerChangeSearch } from '../domain/people-explorer-detail';
export type { PeopleExplorerChangeSearch } from '../domain/people-explorer-detail';
export async function peopleExplorerDetail(actor: WorkContext, selection: OrganizationSelection, tab: string, archived = false, changeSearch: PeopleExplorerChangeSearch = {}) { let write: SqlDatabase; const app = createPeopleExplorerApplication({ prepare: async (a) => { write = (await holdingResources(a.holdingId)).write; }, selection: holdingDirectorySelection, organization: (a, search) => organizationOverview(write, a, search), person: getPerson, directoryOptions, staff: a => staffSettings(write, a), payroll: payrollOptions, lifecycle: a => peopleLifecycleData(write, a), branding: a => listBrandProfiles(write, a), administration: administrationAccess, authority: a => appointmentOverview(write, a), history: id => peopleLevelHistory(write, id) }); try {
    const detail = await app.peopleExplorerDetail(actor, selection, tab, archived, changeSearch);
    return { ...detail, authority: detail.authority as ComponentProps<typeof DepartmentAuthorityWorkspace>['initial'] | null };
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new ApiError(error.status, error.code, error.message);
    throw error;
} }
