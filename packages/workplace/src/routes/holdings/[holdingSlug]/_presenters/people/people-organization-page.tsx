import {forbidden} from 'next/navigation';
import {headers} from 'next/headers';
import {peopleExplorerDetail} from "@erpx/server-people_payroll/public/people-explorer-detail";
import type {OrganizationSelection} from "@erpx/feature-people_payroll/components/organization-explorer";
import { FeaturePage } from "@erpx/feature-shared/components/features/feature-page";
import { PeopleNavigation } from "@erpx/feature-people_payroll/components/people-navigation";
import { PeopleOrganizationWorkspace } from "@erpx/feature-people_payroll/components/people-organization-workspace";
import { PeopleStructureWorkspace } from "@erpx/feature-people_payroll/components/people-structure-workspace";
import { emptyDraft } from "@erpx/feature-organization/lib/organization-draft";
import {
  nativeOrganizationDetail,
  nativeOrganizationFailure,
  nativeOrganizationPage,
} from "@erpx/feature-organization/lib/native-organization";
import { PageHeader } from "@erpx/ui/components/primitives";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, type WorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError } from "@erpx/server-runtime/public/http";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { peopleWorkspaceData } from "@erpx/server-people_payroll/public/people-workspace";
import styles from "@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/people/people-organization-page.module.css";

export async function PeopleOrganizationPage({ actor, search }: {
  actor: WorkContext;
  search: { [key:string]:string|undefined; status?: string; view?: string; person?: string; department?: string; team?: string; tab?: string; edit?: string; changeId?: string; changeState?: string; changeQuery?: string; changePage?: string };
}) {
  const base = workspaceBase(publicScope(actor));
  const { directory, archivedPeople, accessOrganization: retainedOrganization, canManage, canManageStructure,purchases } =
    await peopleWorkspaceData(actor, search).catch(error => pageError(error, actor.holdingSlug));
  // Organization-owned page facts come from the native workspace when it is
  // configured. A configured failure is closed by the route error boundary;
  // only an absent configuration keeps the retained Organization read.
  const nativeOrganization = retainedOrganization
    ? await nativeOrganizationPage(
        actor,
        actor.holdingSlug,
        {
          changeId: search.changeId,
          changeState: search.changeState,
          changeQuery: search.changeQuery,
          changePage: search.changePage,
        },
        await headers(),
        retainedOrganization,
      ).catch(error => {
        const failure = nativeOrganizationFailure(error);
        return pageError(
          failure ? new ApiError(failure.status, failure.code, failure.message) : error,
          actor.holdingSlug,
        );
      })
    : undefined;
  const accessOrganization = nativeOrganization?.access ?? retainedOrganization;
  const kind=["person","company","branch","department","team","position","level","unassigned"].find(kind=>search[kind]);
  if(search.tab==='working-time'&&!canManage)forbidden();
  let initialDetail=kind&&!search.new&&!search.panel?await peopleExplorerDetail(actor,{kind:kind as OrganizationSelection['kind'],id:search[kind]!},search.tab==='working-time'?(kind==='person'?'profile':'overview'):search.tab??(kind==='person'?'profile':'overview'),search.status==='archived',search).catch(error=>pageError(error,actor.holdingSlug)):undefined;
  if (nativeOrganization && initialDetail?.organization) {
    // Keep the retained People/default fields the native workspace cannot
    // represent, but replace every Organization-owned fact.
    initialDetail = {
      ...initialDetail,
      organization: nativeOrganizationDetail(
        nativeOrganization.overview,
        initialDetail.organization,
        nativeOrganization.access.canManage,
      ),
    };
  }
  const canReadPay = hasPermission(actor, "people.compensation.read") || hasPermission(actor, "people.compensation.payroll");
  const workspaceProps = {
    purchases,
    base,
    initialDetail,
    holdingName: actor.holdingName,
    currentCompanyId: actor.companyId,
    actorId: actor.userId,
    data: directory,
    archivedPeople,
    accessOrganization,
    organization: nativeOrganization?.overview ?? null,
    privatePerson: null,
    groups: [],
    editorNodes: [],
    staffOptions: null,
    lifecycle: null,
    history: [],
    branding: null,
    authority: null,
    canManage,
    canManageStructure,
    canReadPay,
    canApprovePay: hasPermission(actor, "people.compensation.approve"),
    canOpenHrSettings: !actor.branchId && !actor.departmentId && (hasPermission(actor, "people.payroll.details") || hasPermission(actor, "leave.policy.manage") || canManage && hasPermission(actor, "organization.manage")),
  };
  const workspace = search.view === "legacy"
    ? <PeopleStructureWorkspace
        key={`${actor.userId}:${actor.companyId}`}
        people={workspaceProps}
        definition={emptyDraft()}
      />
    : <PeopleOrganizationWorkspace {...workspaceProps} />;
  return <FeaturePage className={`content-stack ${styles.page}`} data-source="live">
    <PageHeader appearance="standard" eyebrow="Your team" title="People" description="Choose a unit to manage its positions, people, and team schedule." />
    <PeopleNavigation base={base} actor={actor} scopeKind={actor.scope.kind} active="directory" />
    {workspace}
  </FeaturePage>;
}
