import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { peopleSettingsHref } from "@erpx/feature-people_payroll/lib/people-route-context";
import { peoplePayrollPresentationQueries } from "@erpx/server-people_payroll/public/presentation-queries";
import { PeopleAccessWorkspace } from "@erpx/feature-identity_access/components/people-access-workspace";
import { iamPresentationQueries } from "@erpx/server-identity_access/public/presentation-context-queries";

import { ActionLink as UiActionLink } from "@erpx/ui/components/foundation";
import { BetaPeople } from "@erpx/feature-collaboration/components/people-page";
import { FeaturePage, FeatureWidget, WidgetGrid } from "@erpx/feature-shared/components/features/feature-page";
import { MemberProfile } from "@erpx/feature-identity_access/components/member-profile";
import { PeopleNavigation } from "@erpx/feature-people_payroll/components/people-navigation";
import { PeopleDirectoryWorkspace } from "@erpx/feature-people_payroll/components/people-directory-workspace";
import { ListPagination } from "@erpx/ui/components/list-pagination";
import { PageHeader } from "@erpx/ui/components/primitives";
import { PeopleEditor } from "@erpx/workplace/components/work-editors";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { workspaceBase, workspaceApiHref } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope,requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { pageError } from "@erpx/server-work_projection/public/page-error";

import { canManagePeople,directoryOptions } from "@erpx/server-people_payroll/public/people-directory";
import { peopleDirectoryPage } from "@erpx/server-people_payroll/public/people-directory-page";

import { getPerson } from "@erpx/server-people_payroll/public/people-payroll";


import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { forbidden, redirect } from "next/navigation";

import directory from "@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/people/directory.module.css";
import { PeopleOrganizationPage } from "@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/people/people-organization-page";

export const dynamic = "force-dynamic";

export default async function PeoplePage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{
    panel?: string; editor?: string; unit?: string; company?: string;
    display?: string;
    new?: string;
    person?: string;
    retirement?: string;
    cursor?: string;
    "archived.cursor"?: string; "archived.q"?: string; "archived.retirement"?: string;
    q?: string;
    status?: string;
    level?: string;
    department?: string;
    team?: string;
    view?: string;
    section?: string;
    member?: string;
    position?:string;
    case?:string;
    tab?: string;
    edit?: string;
  }>;
}) {
  const { holdingSlug } = await params;
  const actor = await requireWorkContext(holdingSlug);
  const search = await searchParams;
  if (search.panel === "settings") redirect(peopleSettingsHref(workspaceBase(publicScope(actor)), search));
  if (search.view === "payroll" && !actor.beta) redirect(canonicalUiHref(`${workspaceBase(publicScope(actor))}/people/payroll`));
  if (actor.beta) return <BetaPeople actor={actor} member={search.member} />;
  const base = workspaceBase(publicScope(actor));
  if (!search.view && hasPermission(actor, "iam.read") && !hasPermission(actor, "people.read") && !actor.branchId && !actor.departmentId) redirect(canonicalUiHref(`${base}/people?view=users`));
  if (search.view === "users") {
    if (!hasPermission(actor, "iam.read")) forbidden();
    const {queries,actor:iam} = await iamPresentationQueries(holdingSlug);
    const data = await queries.peopleAccessSnapshot(iam).catch(error=>pageError(error,holdingSlug));
    return <FeaturePage className="content-stack" data-source="live"><PageHeader appearance="standard" eyebrow="People" title="Users & Access" description="Manage internal accounts, external organizations and teams, and scoped access." /><PeopleNavigation base={base} actor={actor} scopeKind={actor.scope.kind} active="users" /><PeopleAccessWorkspace initial={data} initialView={["users","organizations","teams","roles"].includes(search.section??"")?search.section as "users"|"organizations"|"teams"|"roles":"users"} api={workspaceApiHref(publicScope(actor),"iam/people-access")} /></FeaturePage>;
  }
  const archived = search.status === "archived";
  const canManage = canManagePeople(actor);
  if (!canManage && (archived || search.new === "person" || search.member || search.case)) forbidden();
  // Preserve explicit legacy add-person, archived, linked-account, and case flows.
  // The normal directory and all structure selections use the shared explorer.
  if (!search.member && !search.case) {
    return <PeopleOrganizationPage actor={actor} search={search} />;
  }
  const editing = canManage && search.new === "person";
  const editingSelectedPerson = canManage && !!search.person;
  const canExplore = ["people.read","people.manage"].some(p=>hasPermission(actor,p));
  const exploring = canExplore && search.display === "explorer";
  const presentation = await peoplePayrollPresentationQueries(actor.holdingId);
  const [directoryPage, staffOptions, nodes, organization, lifecycle, selectedPersonData] = await Promise.all([
    peopleDirectoryPage(actor,{
      ...search,
      status: archived ? "archived" : "current",
      q: archived ? search["archived.q"] ?? search.q : search.q,
      retirement: archived ? search["archived.retirement"] ?? search.retirement : search.retirement,
      cursor: archived ? search["archived.cursor"] ?? search.cursor : search.cursor,
    }),
    editing || editingSelectedPerson ? presentation.staffSettings(actor) : null,
    editing || editingSelectedPerson ? directoryOptions(actor) : [],
    editing || editingSelectedPerson ? presentation.organizationOverview(actor) : undefined,
    (editing || exploring || search.person || search.member || search.case) && ["people.manage","people.read","iam.read","organization.manage"].some(p=>hasPermission(actor,p)) ? presentation.peopleLifecycleData(actor) : null,
    editingSelectedPerson ? getPerson(actor, search.person!).catch(() => null) : null,
  ]).catch(error => pageError(error, holdingSlug));
  const selectedMember = directoryPage?.members.find(m=>m.id===search.member);
  return (
    <FeaturePage className="content" data-source="live">
      <PageHeader appearance="standard"
        eyebrow="Your team"
        title="People"
        description="Manage people, positions, and employment changes. IT prepares their resources in Technology."
        action={!editing && canManage ? <UiActionLink appearance="standard" variant="primary" href={canonicalUiHref(`${base}/people?new=person`)}>Add person</UiActionLink> : undefined}
      />
      <PeopleNavigation base={base} actor={actor} scopeKind={actor.scope.kind} active="directory"/>
      <div className={directory.directory}>
          {selectedMember && !selectedMember.personId && (
            <MemberProfile actor={actor} base={base} member={selectedMember} />
          )}
          {editing && staffOptions && (
            <PeopleEditor appearance="standard"
              initialOpen
              initialPositionId={organization?.positions.some(p=>p.id===search.position)?search.position:undefined}
              base={base}
              nodes={nodes}
              levels={staffOptions.levels}
              people={staffOptions.people}
              organization={organization}
              gpoPositions={lifecycle?.positions}
            />
          )}
          <nav className={directory.viewTabs} data-feature-tabs aria-label="People directory status">
            <Link href={canonicalUiHref(`${base}/people`)} aria-current={!archived ? "page" : undefined}>Current</Link>
            <Link href={canonicalUiHref(`${base}/people?status=archived`)} aria-current={archived ? "page" : undefined}>Archived</Link>
          </nav>
          <WidgetGrid>
            {directoryPage && <FeatureWidget appearance="standard" id={archived ? "archived-profiles" : exploring ? "people-explorer" : "current-profiles"} title={archived ? "Archived profiles" : exploring ? "People explorer" : "Current profiles"}>
              <PeopleDirectoryWorkspace
                base={base}
                lifecycle={archived ? null : lifecycle}
                selectedRow={directoryPage.selected}
                initialCaseId={search.case}
                initialMemberId={search.member}
                initialPositionId={search.position}
                rows={directoryPage.items}
                canManage={canManage}
                hasExplorer={!archived && canExplore}
                editor={staffOptions && organization && directoryPage.selected && (selectedPersonData || !directoryPage.selected.personId) ? {
                  person: selectedPersonData?.person,
                  member: !directoryPage.selected.personId ? {
                    id: directoryPage.selected.userId ?? directoryPage.selected.id,
                    name: directoryPage.selected.name,
                    email: directoryPage.selected.email,
                  } : undefined,
                  nodes,
                  levels: staffOptions.levels,
                  people: staffOptions.people,
                  organization,
                  levelManagedSeparately: staffOptions.canManage,
                } : null}
              />
              <ListPagination {...directoryPage} query={search} count={directoryPage.items.length}/>
            </FeatureWidget>}
          </WidgetGrid>


      </div>
    </FeaturePage>
  );
}
