import type { ComponentProps } from "react";
import type { OrganizationAccessData } from "@erpx/feature-organization/lib/organization-access";
import type { OrganizationOverview } from "@erpx/feature-organization/lib/organization";
import type { PeopleLifecycleData } from "@erpx/feature-people_payroll/lib/people-lifecycle";
import type { HoldingPeopleDirectory } from "@erpx/server-organization/public/holding-people-directory";
import type { getPerson } from "@erpx/server-people_payroll/public/people-payroll";
import type { DepartmentAuthorityWorkspace } from "@erpx/feature-organization/components/department-authority-workspace";
import type { BrandAdministration } from "@erpx/feature-brand/components/brand-administration";

type PersonDetail = Awaited<ReturnType<typeof getPerson>>;
type Row = Record<string, unknown>;
export type Authority = ComponentProps<typeof DepartmentAuthorityWorkspace>["initial"];
type Branding = ComponentProps<typeof BrandAdministration>;

export type PeopleOrganizationProps = {
  purchases?: import("@erpx/feature-people_payroll/lib/purchase-controls").PeoplePurchaseControls;
  base: string;
  holdingName: string;
  currentCompanyId: string;
  actorId: string;
  data: HoldingPeopleDirectory;
  archivedPeople: HoldingPeopleDirectory["people"];
  accessOrganization: OrganizationAccessData | null;
  organization: OrganizationOverview | null;
  privatePerson: PersonDetail | null;
  groups: Row[];
  editorNodes: Row[];
  staffOptions: { levels: Row[]; people: Row[]; canManage: boolean } | null;
  lifecycle: PeopleLifecycleData | null;
  history: Row[];
  branding: Branding | null;
  authority: Authority | null;
  canManage: boolean;
  canManageStructure: boolean;
  canReadPay: boolean;
  canApprovePay: boolean;
  canOpenHrSettings?: boolean;
  initialDetail?: Awaited<ReturnType<typeof import("@erpx/server-people_payroll/public/people-explorer-detail").peopleExplorerDetail>>;
};

