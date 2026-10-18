import {forbidden} from 'next/navigation';
import {peopleExplorerDetail} from "@/server/modules/people_payroll/public/people-explorer-detail";
import type {OrganizationSelection} from "@/components/people_payroll/organization-explorer";
import { FeaturePage } from "@/components/shared/features/feature-page";
import { PeopleNavigation } from "@/components/people_payroll/people-navigation";
import { PeopleOrganizationWorkspace } from "@/components/people_payroll/people-organization-workspace";
import { PageHeader } from "@/components/ui/primitives";
import { hasPermission } from "@/lib/identity_access/permissions";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { publicScope, type WorkContext } from "@/server/modules/identity_access/public/authorization";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { peopleWorkspaceData } from "@/server/modules/people_payroll/public/people-workspace";
import styles from "@/app/holdings/[holdingSlug]/_presenters/people/people-organization-page.module.css";

export async function PeopleOrganizationPage({ actor, search }: {
  actor: WorkContext;
  search: { [key:string]:string|undefined; status?: string; person?: string; department?: string; team?: string; tab?: string; edit?: string; changeId?: string; changeState?: string; changeQuery?: string; changePage?: string };
}) {
  const base = workspaceBase(publicScope(actor));
  const { directory, archivedPeople, accessOrganization, canManage, canManageStructure,purchases } =
    await peopleWorkspaceData(actor, search).catch(error => pageError(error, actor.holdingSlug));
  const kind=["person","company","branch","department","team","position","level","unassigned"].find(kind=>search[kind]);
  if(search.tab==='working-time'&&!canManage)forbidden();
  const initialDetail=kind&&!search.new&&!search.panel?await peopleExplorerDetail(actor,{kind:kind as OrganizationSelection['kind'],id:search[kind]!},search.tab==='working-time'?(kind==='person'?'profile':'overview'):search.tab??(kind==='person'?'profile':'overview'),search.status==='archived',search).catch(error=>pageError(error,actor.holdingSlug)):undefined;
  const canReadPay = hasPermission(actor, "people.compensation.read") || hasPermission(actor, "people.compensation.payroll");
  return <FeaturePage className={`content-stack ${styles.page}`} data-source="live">
    <PageHeader appearance="standard" eyebrow="Your team" title="People" description="Choose a unit to manage its positions, people, and team schedule." />
    <PeopleNavigation base={base} actor={actor} scopeKind={actor.scope.kind} active="directory" />
    <PeopleOrganizationWorkspace
      purchases={purchases}
      base={base}
      initialDetail={initialDetail}
      holdingName={actor.holdingName}
      currentCompanyId={actor.companyId}
      actorId={actor.userId}
      data={directory}
      archivedPeople={archivedPeople}
      accessOrganization={accessOrganization}
      organization={null}
      privatePerson={null}
      groups={[]}
      editorNodes={[]}
      staffOptions={null}
      lifecycle={null}
      history={[]}
      branding={null}
      authority={null}
      canManage={canManage}
      canManageStructure={canManageStructure}
      canReadPay={canReadPay}
      canApprovePay={hasPermission(actor, "people.compensation.approve")}
      canOpenHrSettings={!actor.branchId && !actor.departmentId && (hasPermission(actor, "people.payroll.details") || hasPermission(actor, "leave.policy.manage") || canManage && hasPermission(actor, "organization.manage"))}
    />
  </FeaturePage>;
}
