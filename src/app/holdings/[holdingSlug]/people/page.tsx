
import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { BetaPeople } from "@/components/beta/people-page";
import { FeaturePage, FeatureWidget, WidgetGrid } from "@/components/features/feature-page";
import { MemberProfile } from "@/components/modules/member-profile";
import { PeopleNavigation } from "@/components/modules/people-navigation";
import { PeopleDirectory } from "@/components/people/people-directory";
import { ListPagination } from "@/components/ui/list-pagination";
import { PageHeader } from "@/components/ui/primitives";
import { PeopleEditor } from "@/components/workplace/work-editors";
import { hasPermission } from "@/lib/permissions";
import { workspaceBase } from "@/lib/workspace";
import { publicScope,requireWorkContext } from "@/server/authorization";
import { pageError } from "@/server/page-error";
import { organizationOverview } from "@/server/organization";
import { canManagePeople,directoryOptions } from "@/server/people-directory";
import { peopleDirectoryPage } from "@/server/people-directory-page";
import { peopleLifecycleData } from "@/server/people-lifecycle";
import { getPerson } from "@/server/people-payroll";
import { holdingResources } from "@/server/runtime";
import { staffSettings } from "@/server/staff-settings";
import { RecordLink as Link } from "@/components/records/record-link";
import { forbidden, redirect } from "next/navigation";

import directory from "./directory.module.css";
import { PeopleOrganizationPage } from "./people-organization-page";

export const dynamic = "force-dynamic";

export default async function PeoplePage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{
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
  if (search.view === "payroll" && !actor.beta) redirect(`${workspaceBase(publicScope(actor))}/people/payroll`);
  if (actor.beta) return <BetaPeople actor={actor} member={search.member} />;
  const base = workspaceBase(publicScope(actor));
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
  const { write } = await holdingResources(actor.holdingId);
  const [directoryPage, staffOptions, nodes, organization, lifecycle, selectedPersonData] = await Promise.all([
    peopleDirectoryPage(actor,{
      ...search,
      status: archived ? "archived" : "current",
      q: archived ? search["archived.q"] ?? search.q : search.q,
      retirement: archived ? search["archived.retirement"] ?? search.retirement : search.retirement,
      cursor: archived ? search["archived.cursor"] ?? search.cursor : search.cursor,
    }),
    editing || editingSelectedPerson ? staffSettings(write,actor) : null,
    editing || editingSelectedPerson ? directoryOptions(actor) : [],
    editing || editingSelectedPerson ? organizationOverview(write,actor) : undefined,
    (editing || exploring || search.person || search.member || search.case) && ["people.manage","people.read","iam.read","organization.manage"].some(p=>hasPermission(actor,p)) ? peopleLifecycleData(write,actor) : null,
    editingSelectedPerson ? getPerson(actor, search.person!).catch(() => null) : null,
  ]).catch(error => pageError(error, holdingSlug));
  const selectedMember = directoryPage?.members.find(m=>m.id===search.member);
  return (
    <FeaturePage className="content" data-source="live">
      <PageHeader appearance="standard"
        eyebrow="Your team"
        title="People"
        description="Manage people, positions, and employment changes. IT prepares their resources in Technology."
        action={!editing && canManage ? <UiActionLink appearance="standard" variant="primary" href={`${base}/people?new=person`}>Add person</UiActionLink> : undefined}
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
            <Link href={`${base}/people`} aria-current={!archived ? "page" : undefined}>Current</Link>
            <Link href={`${base}/people?status=archived`} aria-current={archived ? "page" : undefined}>Archived</Link>
          </nav>
          <WidgetGrid>
            {directoryPage && <FeatureWidget appearance="legacy" id={archived ? "archived-profiles" : exploring ? "people-explorer" : "current-profiles"} title={archived ? "Archived profiles" : exploring ? "People explorer" : "Current profiles"}>
              <PeopleDirectory
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
