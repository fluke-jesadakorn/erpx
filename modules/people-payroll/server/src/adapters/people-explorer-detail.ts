import type { ComponentProps } from "react";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { OrganizationSelection } from "@erpx/people-payroll-ui/components/organization-explorer";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import type { DepartmentAuthorityWorkspace } from "@erpx/organization-ui/components/department-authority-workspace";
import { administrationAccess } from "@erpx/platform-runtime-server/public/administration";
import { nativeListBrandProfiles } from "@erpx/brand-server/public/branding";
import {appointmentOverview} from '@erpx/organization-server/public/department-authority-reads';
import { holdingDirectorySelection } from "@erpx/organization-server/public/holding-people-directory";
import {organizationOverview} from '@erpx/organization-server/public/organization';
import { directoryOptions } from "@erpx/people-payroll-server/adapters/people-directory";
import { peopleLifecycleData } from "@erpx/people-payroll-server/adapters/people-lifecycle";
import { getPerson, nativePayGroups } from "@erpx/people-payroll-server/adapters/people-payroll";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import { staffSettings } from "@erpx/operations-server/public/staff-settings";
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { BusinessRuleError } from '@erpx/platform-shared-server/index';
import { peopleLevelHistory } from '@erpx/operations-server/public/people-level-history';
import { createPeopleExplorerApplication } from '@erpx/people-payroll-server/application/people-explorer-detail';
import type { PeopleExplorerChangeSearch } from '@erpx/people-payroll-server/domain/people-explorer-detail';
export type { PeopleExplorerChangeSearch } from '@erpx/people-payroll-server/domain/people-explorer-detail';
export async function peopleExplorerDetail(actor: WorkContext, selection: OrganizationSelection, tab: string, archived = false, changeSearch: PeopleExplorerChangeSearch = {}) { let write: SqlDatabase; const app = createPeopleExplorerApplication({ prepare: async (a) => { write = (await holdingResources(a.holdingId)).write; }, selection: holdingDirectorySelection, organization: (a, search) => organizationOverview(write, a, search), person: getPerson, directoryOptions, staff: a => staffSettings(write, a), payroll: nativePayGroups, lifecycle: a => peopleLifecycleData(write, a), branding: a => nativeListBrandProfiles(a), administration: administrationAccess, authority: a => appointmentOverview(write, a), history: async id => (await peopleLevelHistory(write, id, actor)).map(row => ({ ...row })) }); try {
    const detail = await app.peopleExplorerDetail(actor, selection, tab, archived, changeSearch);
    return { ...detail, authority: detail.authority as ComponentProps<typeof DepartmentAuthorityWorkspace>['initial'] | null };
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new ApiError(error.status, error.code, error.message);
    throw error;
} }
