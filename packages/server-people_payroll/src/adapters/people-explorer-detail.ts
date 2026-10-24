import type { ComponentProps } from "react";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { OrganizationSelection } from "@erpx/feature-people_payroll/components/organization-explorer";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { DepartmentAuthorityWorkspace } from "@erpx/feature-organization/components/department-authority-workspace";
import { administrationAccess } from "@erpx/server-runtime/public/administration";
import { listBrandProfiles } from "@erpx/server-brand/public/branding";
import {appointmentOverview} from '@erpx/server-organization/public/department-authority-reads';
import { holdingDirectorySelection } from "@erpx/server-organization/public/holding-people-directory";
import {organizationOverview} from '@erpx/server-organization/public/organization';
import { directoryOptions } from "@erpx/server-people_payroll/adapters/people-directory";
import { peopleLifecycleData } from "@erpx/server-people_payroll/adapters/people-lifecycle";
import { getPerson, payrollOptions } from "@erpx/server-people_payroll/adapters/people-payroll";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { staffSettings } from "@erpx/server-operations/public/staff-settings";
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { peopleLevelHistory } from '@erpx/server-operations/public/people-level-history';
import { createPeopleExplorerApplication } from '@erpx/server-people_payroll/application/people-explorer-detail';
import type { PeopleExplorerChangeSearch } from '@erpx/server-people_payroll/domain/people-explorer-detail';
export type { PeopleExplorerChangeSearch } from '@erpx/server-people_payroll/domain/people-explorer-detail';
export async function peopleExplorerDetail(actor: WorkContext, selection: OrganizationSelection, tab: string, archived = false, changeSearch: PeopleExplorerChangeSearch = {}) { let write: SqlDatabase; const app = createPeopleExplorerApplication({ prepare: async (a) => { write = (await holdingResources(a.holdingId)).write; }, selection: holdingDirectorySelection, organization: (a, search) => organizationOverview(write, a, search), person: getPerson, directoryOptions, staff: a => staffSettings(write, a), payroll: payrollOptions, lifecycle: a => peopleLifecycleData(write, a), branding: a => listBrandProfiles(write, a), administration: administrationAccess, authority: a => appointmentOverview(write, a), history: id => peopleLevelHistory(write, id) }); try {
    const detail = await app.peopleExplorerDetail(actor, selection, tab, archived, changeSearch);
    return { ...detail, authority: detail.authority as ComponentProps<typeof DepartmentAuthorityWorkspace>['initial'] | null };
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new ApiError(error.status, error.code, error.message);
    throw error;
} }
