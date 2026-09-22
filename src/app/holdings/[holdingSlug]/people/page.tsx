import { BetaPeople } from "@/components/beta/people-page";
import { FeaturePage, FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { MemberProfile } from "@/components/modules/member-profile";
import { PayGroupEditor,PayrollRunEditor,PayrollAdvanceConsole } from "@/components/modules/payroll-controls";
import { PeopleNavigation } from "@/components/modules/people-navigation";
import { PeopleDirectory } from "@/components/people/people-directory";
import { ListPagination } from "@/components/ui/list-pagination";
import {
EmptyState,
PageHeader,
StatusBadge,
} from "@/components/ui/primitives";
import styles from "@/components/workplace/pages.module.css";
import { PeopleEditor } from "@/components/workplace/work-editors";
import { hasPermission } from "@/lib/permissions";
import { workspaceBase } from "@/lib/workspace";
import { publicScope,requireWorkContext } from "@/server/authorization";
import { organizationOverview } from "@/server/organization";
import { canManagePeople,directoryOptions } from "@/server/people-directory";
import { peopleDirectoryPage } from "@/server/people-directory-page";
import { peopleLifecycleData } from "@/server/people-lifecycle";
import {
payrollOptions,
payrollRunsPage,
payrollAdvancePage,
getPerson,
} from "@/server/people-payroll";
import { holdingResources } from "@/server/runtime";
import { staffSettings } from "@/server/staff-settings";
import { ArrowUpRight } from "lucide-react";
import Link from "next/link";

import directory from "./directory.module.css";

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
    view?: string;
    member?: string;
    position?:string;
    case?:string;
  }>;
}) {
  const { holdingSlug } = await params;
  const actor = await requireWorkContext(holdingSlug);
  const search = await searchParams;
  if (actor.beta) return <BetaPeople actor={actor} member={search.member} />;
  const base = workspaceBase(publicScope(actor));
  const canPayroll = ["holding_owner","hr_admin","people_manager","payroll_preparer","payroll_approver","auditor"].includes(actor.role) && !actor.branchId && !actor.departmentId;
  const canPayrollAdvance = hasPermission(actor,"self.requests");
  const payroll = search.view === "payroll" && (canPayroll || canPayrollAdvance);
  const archived = search.status === "archived";
  const canManage = canManagePeople(actor);
  const editing = !payroll && canManage && (search.new === "person" || !!search.position);
  const editingSelectedPerson = !payroll && canManage && !!search.person;
  const canExplore = !payroll && ["people.read","people.manage"].some(p=>hasPermission(actor,p));
  const exploring = canExplore && search.display === "explorer";
  const { write } = await holdingResources(actor.holdingId);
  const [directoryPage, runsPage, advanceData, staffOptions, nodes, organization, lifecycle, selectedPersonData] = await Promise.all([
    !payroll ? peopleDirectoryPage(actor,{
      ...search,
      status: archived ? "archived" : "current",
      q: archived ? search["archived.q"] ?? search.q : search.q,
      retirement: archived ? search["archived.retirement"] ?? search.retirement : search.retirement,
      cursor: archived ? search["archived.cursor"] ?? search.cursor : search.cursor,
    }) : null,
    payroll && canPayroll ? payrollRunsPage(actor,search.cursor) : null,
    payroll ? payrollAdvancePage(actor).catch(() => ({items:[],options:[]})) : null,
    editing || editingSelectedPerson ? staffSettings(write,actor) : null,
    editing || editingSelectedPerson ? directoryOptions(actor) : [],
    editing || editingSelectedPerson ? organizationOverview(write,actor) : undefined,
    !payroll && (exploring || search.person || search.member || search.case) && ["people.manage","people.read","iam.read","organization.manage"].some(p=>hasPermission(actor,p)) ? peopleLifecycleData(write,actor) : null,
    editingSelectedPerson ? getPerson(actor, search.person!).catch(() => null) : null,
  ]);
  const runs=runsPage?.items??[];
  const selectedMember = directoryPage?.members.find(m=>m.id===search.member);
  return (
    <FeaturePage className="content" data-source="live">
      <PageHeader
        eyebrow="Your team"
        title="People"
        description="Manage people, positions, and employment changes. IT prepares their resources in Technology."
        action={!payroll && !editing && canManage ? <Link className="button" href={`${base}/people?new=person`}>Add person</Link> : undefined}
      />
      <PeopleNavigation base={base} actor={actor} scopeKind={actor.scope.kind} active={payroll ? "payroll" : "directory"} canPayroll={canPayroll || canPayrollAdvance}/>
      {!payroll && (
        <div className={directory.directory}>
          {selectedMember && !selectedMember.personId && (
            <MemberProfile actor={actor} base={base} member={selectedMember} />
          )}
          {editing && staffOptions && (
            <PeopleEditor
              initialOpen
              initialPositionId={organization?.positions.some(p=>p.id===search.position)?search.position:undefined}
              base={base}
              nodes={nodes}
              levels={staffOptions.levels}
              people={staffOptions.people}
              organization={organization}
            />
          )}
          <nav className={directory.viewTabs} aria-label="People directory status">
            <Link href={`${base}/people`} aria-current={!archived ? "page" : undefined}>Current</Link>
            <Link href={`${base}/people?status=archived`} aria-current={archived ? "page" : undefined}>Archived</Link>
          </nav>
          <WidgetGrid>
            {directoryPage && <FeatureWidget id={archived ? "archived-profiles" : exploring ? "people-explorer" : "current-profiles"} title={archived ? "Archived profiles" : exploring ? "People explorer" : "Current profiles"}>
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
      )}
      {payroll && (
        <WidgetGrid pageKey="people-payroll">
          <p className="notice">
            Gross-pay preview only. Statutory calculations and payment adapters
            are not yet available.
          </p>
          {hasPermission(actor,"payroll.prepare") &&
            !actor.branchId && (
              <><FeatureWidget id="payroll-pay-group-policy" title="Pay group policy"><PayGroupEditor base={base}/></FeatureWidget><FeatureWidget id="payroll-new-run" title="Create payroll run"><PayrollRunEditor
                base={base}
                groups={await payrollOptions(actor)}
              /></FeatureWidget></>
          )}
          {canPayroll && <FeatureWidget id="payroll-runs" title="Recent payroll runs"><section className="card" id="payroll-runs-content" style={{ marginTop: 24 }}>
            <div className={styles.panelHead}>
              <h2>Recent payroll runs</h2>
              <span className="badge">{runs.length}</span>
            </div>
            {runs.length === 0 ? (
              <EmptyState
                title="No payroll runs"
                description="Create a pay group and a preview run when your inputs are ready."
              />
            ) : (
              runs.map((run) => (
                <Link
                  className="row"
                  href={`${base}/people/payroll/${encodeURIComponent(String(run.id))}`}
                  key={String(run.id)}
                >
                  <div className="row-main">
                    <div className="row-title">
                      {String(run.period_start)} → {String(run.period_end)}
                    </div>
                    <div className={styles.subline}>
                      {String(run.currency)} · {String(run.mode)} ·{" "}
                      {String(run.revision)} revision
                    </div>
                  </div>
                  <StatusBadge status={String(run.status)} />
                  <ArrowUpRight size={14} />
                </Link>
              ))
            )}
            {runsPage&&<ListPagination {...runsPage} query={search} count={runs.length}/>}
          </section></FeatureWidget>}
          <PayrollAdvanceConsole base={base} data={advanceData ?? {items:[],options:[]}} userId={actor.userId} canPrepare={hasPermission(actor,"payroll.prepare")} canApprove={hasPermission(actor,"payroll.approve")}/>
        </WidgetGrid>
      )}
    </FeaturePage>
  );
}
