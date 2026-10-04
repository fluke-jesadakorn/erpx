import { FeaturePage } from "@/components/features/feature-page";
import { PeopleNavigation } from "@/components/modules/people-navigation";
import { PeopleOrganizationWorkspace } from "@/components/people/people-organization-workspace";
import { PageHeader } from "@/components/ui/primitives";
import { hasPermission } from "@/lib/permissions";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, type WorkContext } from "@/server/authorization";
import { pageError } from "@/server/page-error";
import { peopleWorkspaceData } from "@/server/people-workspace";
import styles from "./people-organization-page.module.css";
import Link from "next/link";

export async function PeopleOrganizationPage({ actor, search }: {
  actor: WorkContext;
  search: { status?: string; person?: string; department?: string; team?: string; tab?: string; edit?: string; changeId?: string; changeState?: string; changeQuery?: string; changePage?: string };
}) {
  const base = workspaceBase(publicScope(actor));
  const { directory, archivedPeople, accessOrganization, canManage, canManageStructure } =
    await peopleWorkspaceData(actor, search).catch(error => pageError(error, actor.holdingSlug));
  const canReadPay = hasPermission(actor, "people.compensation.read") || hasPermission(actor, "people.compensation.payroll");
  return <FeaturePage className={`content-stack ${styles.page}`} data-source="live">
    <PageHeader appearance="standard" eyebrow="Your team" title="People" description="Choose a unit to manage its positions, people, and team schedule." />
    <PeopleNavigation base={base} actor={actor} scopeKind={actor.scope.kind} active="directory" />
    {canManage&&!actor.branchId&&!actor.departmentId&&<Link href={`${base}/people/hire-handoffs`}>Accepted Talent offers → HR onboarding</Link>}
    <PeopleOrganizationWorkspace
      base={base}
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
