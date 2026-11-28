import type { ComponentProps } from "react";
import type { OrganizationAccessData } from "@erpx/organization-ui/lib/organization-access";
import type { OrganizationOverview } from "@erpx/organization-ui/lib/organization";
import type { PeopleLifecycleData } from "@erpx/people-payroll-ui/lib/people-lifecycle";
import type { HoldingPeopleDirectory } from "@erpx/organization-server/public/holding-people-directory";
import type { getPerson } from "@erpx/people-payroll-server/public/people-payroll";
import type { DepartmentAuthorityWorkspace } from "@erpx/organization-ui/components/department-authority-workspace";
import type { BrandAdministration } from "@erpx/brand-ui/components/brand-administration";

type PersonDetail = Awaited<ReturnType<typeof getPerson>>;
type Row = Record<string, unknown>;
export type Authority = ComponentProps<typeof DepartmentAuthorityWorkspace>["initial"];
type Branding = ComponentProps<typeof BrandAdministration>;

export type PeopleOrganizationProps = {
  purchases?: import("@erpx/people-payroll-ui/lib/purchase-controls").PeoplePurchaseControls;
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
  initialDetail?: Awaited<ReturnType<typeof import("@erpx/people-payroll-server/public/people-explorer-detail").peopleExplorerDetail>>;
};

