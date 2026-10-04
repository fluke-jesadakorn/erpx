"use client";
import { ActionLink as UiActionLink } from "@/components/ui/foundation";

import { Button } from "@/components/ui/foundation";

import { TimeText } from "@/components/time/time-preference-context";

import { useState, type ComponentProps, type ReactNode } from "react";
import { useSearchParams } from "next/navigation";
import Link from "next/link";
import dynamic from "next/dynamic";
import type { OrganizationAccessData } from "@/lib/organization-access";
import type { OrganizationOverview } from "@/lib/organization";
import type { PeopleLifecycleData } from "@/lib/people-lifecycle";
import type { HoldingPeopleDirectory } from "@/server/holding-people-directory";
import type { getPerson } from "@/server/people-payroll";
import {
  type OrganizationExplorerContext,
  type OrganizationSelection,
} from "./organization-explorer";
import { OrganizationDraftWorkspace } from "./organization-draft-workspace";
import { PeopleStructureWorkspace } from "./people-structure-workspace";
import { emptyDraft } from "@/lib/organization-draft";
const readOnlyDefinition = emptyDraft();
import styles from "./people-organization-workspace.module.css";

const BrandAdministration = dynamic(() =>
  import("@/components/modules/brand-administration").then((module) => module.BrandAdministration),
);
const DepartmentAuthorityWorkspace = dynamic(() =>
  import("@/components/modules/department-authority-workspace").then(
    (module) => module.DepartmentAuthorityWorkspace,
  ),
);
const OrganizationAdvancedTools = dynamic(() =>
  import("@/components/modules/organization-workspace").then(
    (module) => module.OrganizationAdvancedTools,
  ),
);
const OrganizationChangeQueue = dynamic(() =>
  import("@/components/modules/organization-workspace").then(
    (module) => module.OrganizationChangeQueue,
  ),
);
const OrganizationUnits = dynamic(() =>
  import("@/components/modules/organization-workspace").then((module) => module.OrganizationUnits),
);
const PositionDefaultPay = dynamic(() =>
  import("@/components/modules/organization-workspace").then((module) => module.PositionDefaultPay),
);
const PositionEditor = dynamic(() =>
  import("@/components/modules/organization-workspace").then((module) => module.PositionEditor),
);
const PeopleEditor = dynamic(() =>
  import("@/components/modules/people-editor").then((module) => module.PeopleEditor),
);
const CompensationEditor = dynamic(() =>
  import("@/components/modules/payroll-controls").then((module) => module.CompensationEditor),
);
const LifecyclePanel = dynamic(() =>
  import("@/components/people/lifecycle-workspace").then((module) => module.LifecyclePanel),
);

type PersonDetail = Awaited<ReturnType<typeof getPerson>>;
type Row = Record<string, unknown>;
type Authority = ComponentProps<typeof DepartmentAuthorityWorkspace>["initial"];
type Branding = ComponentProps<typeof BrandAdministration>;

export type PeopleOrganizationProps = {
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
};

function Facts({ values }: { values: [string, ReactNode][] }) {
  return (
    <dl className={styles.facts}>
      {values.map(([label, value]) => (
        <div key={label}>
          <dt>{label}</dt>
          <dd>{value ?? "Not provided"}</dd>
        </div>
      ))}
    </dl>
  );
}

export function PeopleOrganizationDetail(
  props: PeopleOrganizationProps & {
    selection: OrganizationSelection;
    tab: string;
    context: OrganizationExplorerContext;
  },
) {
  const {
    base,
    currentCompanyId,
    actorId,
    data,
    archivedPeople,
    accessOrganization,
    organization,
    privatePerson,
    groups,
    editorNodes,
    staffOptions,
    lifecycle,
    history,
    branding,
    authority,
    canManage,
    canManageStructure,
    canReadPay,
    canApprovePay,
  } = props;
  const query = useSearchParams();
  const isArchived = canManage && query.get("status") === "archived";
  const [editPerson, setEditPerson] = useState(false);
  const [editPosition, setEditPosition] = useState(false);

  const canManageCompany = (companyId: string) =>
    companyId === currentCompanyId && canManageStructure && !!accessOrganization?.canManage;
  const selectedPrivate = (selection: OrganizationSelection) => {
    if (selection.kind !== "person") return null;
    const publicPerson = [...data.people, ...archivedPeople].find(
      (item) => item.id === selection.id,
    );
    return publicPerson?.companyId === currentCompanyId &&
      privatePerson?.person.id === publicPerson.personId &&
      (privatePerson?.person.engagement_id ?? "") === publicPerson.engagementId
      ? privatePerson
      : null;
  };
  const selectedCompany = (selection: OrganizationSelection) => {
    if (selection.kind === "company" || selection.kind === "unassigned") return selection.id;
    if (selection.kind === "person")
      return [...data.people, ...archivedPeople].find((item) => item.id === selection.id)
        ?.companyId;
    if (selection.kind === "position")
      return data.positions.find((item) => item.id === selection.id)?.companyId;
    if (selection.kind === "level")
      return data.levels.find((item) => item.id === selection.id)?.companyId;
    return data.nodes.find((item) => item.id === selection.id)?.companyId;
  };
  const inManagedCompany = (selection: OrganizationSelection) =>
    canManageCompany(selectedCompany(selection) ?? "");

  function renderDetail(
    selection: OrganizationSelection,
    tab: string,
    context: OrganizationExplorerContext,
  ): ReactNode | undefined {
    const local = selectedCompany(selection) === currentCompanyId;
    const privateData = selectedPrivate(selection);
    const publicPerson =
      selection.kind === "person"
        ? [...data.people, ...archivedPeople].find((item) => item.id === selection.id)
        : undefined;
    const position =
      selection.kind === "position"
        ? data.positions.find((item) => item.id === selection.id)
        : undefined;
    const hrPosition =
      local && position
        ? organization?.positions.find((item) => item.id === position.id)
        : undefined;

    if (
      selection.kind === "person" &&
      publicPerson &&
      tab === "Profile" &&
      privateData &&
      canManage
    ) {
      const person = privateData.person;
      return (
        <div className={styles.stack}>
          <div className={styles.actionRow}>
            <Button
              variant="secondary"
              type="button"
              onClick={() => setEditPerson((value) => !value)}
            >
              {editPerson ? "Close editor" : "Edit profile"}
            </Button>
            <Link href={`${base}/people/${encodeURIComponent(publicPerson.personId)}`}>
              Full HR record and documents
            </Link>
            {publicPerson.userId && (
              <Link href={`${base}/profiles/${encodeURIComponent(publicPerson.userId)}`}>
                Member profile
              </Link>
            )}
          </div>
          <Facts
            values={[
              ["Legal name", String(person.legal_name ?? publicPerson.legalName)],
              [
                "Preferred name",
                person.preferred_name ? String(person.preferred_name) : "Not provided",
              ],
              ["Work email", publicPerson.workEmail ?? "Not provided"],
              ["Work phone", publicPerson.workPhone ?? "Not provided"],
              ["Private email (HR)", person.email ? String(person.email) : "Not provided"],
              ["Private phone (HR)", person.phone ? String(person.phone) : "Not provided"],
            ]}
          />
          {editPerson && staffOptions && organization && (
            <PeopleEditor
              key={publicPerson.engagementId}
              base={base}
              person={person}
              nodes={editorNodes}
              levels={staffOptions.levels}
              people={staffOptions.people}
              organization={organization}
              levelManagedSeparately={staffOptions.canManage}
              initialOpen
              alwaysOpen
              returnTo={`${base}/people?person=${encodeURIComponent(publicPerson.engagementId)}`}
            />
          )}
        </div>
      );
    }
    if (selection.kind === "person" && publicPerson && tab === "Profile") {
      return (
        <div className={styles.stack}>
          <Facts
            values={[
              ["Name", publicPerson.legalName],
              ["Position", publicPerson.jobTitle],
              ["Work email", publicPerson.workEmail ?? "Not provided"],
              ["Work phone", publicPerson.workPhone ?? "Not provided"],
              ["Company", context.company?.name ?? "Not provided"],
            ]}
          />
          {publicPerson.userId && (
            <UiActionLink
              variant="secondary"
              href={`${base}/profiles/${encodeURIComponent(publicPerson.userId)}`}
            >
              Open member profile
            </UiActionLink>
          )}
        </div>
      );
    }
    if (
      selection.kind === "person" &&
      publicPerson &&
      tab === "Employment" &&
      privateData &&
      canManage
    ) {
      const person = privateData.person;
      return (
        <div className={styles.stack}>
          <Facts
            values={[
              ["Employment type", String(person.employment_type ?? "Not provided")],
              ["Worker type", String(person.worker_type ?? "Not provided")],
              ["Start date", String(person.start_date ?? "Not provided")],
              ["Status", String(person.engagement_status ?? "Not provided")],
              [
                "Position",
                context.positions.find((item) => item.id === publicPerson.positionId)?.title ??
                  publicPerson.jobTitle,
              ],
              ["Company", context.company?.name ?? "Not provided"],
              ["Level", String(person.level_name ?? "Not assigned")],
              [
                "Manager",
                organization?.people.find((item) => item.person_id === person.manager_person_id)
                  ?.legal_name ?? "Not assigned",
              ],
            ]}
          />
          {lifecycle && Boolean(person.engagement_id) && (
            <LifecyclePanel
              base={base}
              data={lifecycle}
              person={{
                id: publicPerson.personId,
                engagementId: publicPerson.engagementId,
                positionId: publicPerson.positionId,
                managerPersonId: person.manager_person_id ? String(person.manager_person_id) : null,
                name: publicPerson.legalName,
                status: String(person.engagement_status ?? "active"),
              }}
            />
          )}
        </div>
      );
    }
    if (selection.kind === "person" && tab === "Compensation" && privateData && canReadPay) {
      return privateData.privateAccess ? (
        <CompensationEditor
          base={base}
          person={privateData.person}
          groups={groups}
          revisions={privateData.revisions}
          canEdit={canManage}
          canApprove={canApprovePay}
          userId={actorId}
        />
      ) : (
        <p className={styles.muted}>
          Compensation is restricted to authorized HR and Payroll roles.
        </p>
      );
    }
    if (selection.kind === "person" && tab === "History" && privateData && canManage) {
      return (
        <div className={styles.stack}>
          <h3>Employment changes</h3>
          {history.length ? (
            <ol className={styles.history}>
              {history.map((item, index) => (
                <li key={`${String(item.created_at)}:${index}`}>
                  <strong>{String(item.level_name ?? "Level change")}</strong>
                  <span>
                    {item.previous_level_name ? `From ${String(item.previous_level_name)} · ` : ""}
                    {<TimeText value={Number(item.created_at)} />}
                  </span>
                  <p>{String(item.reason ?? "No reason recorded")}</p>
                </li>
              ))}
            </ol>
          ) : (
            <p className={styles.muted}>No level changes recorded.</p>
          )}
          {lifecycle && (
            <p>
              {lifecycle.cases.filter((item) => item.person_id === publicPerson?.personId).length}{" "}
              lifecycle cases recorded.{" "}
              <Link
                href={`${base}/people/${encodeURIComponent(publicPerson?.personId ?? "")}?tab=work`}
              >
                Open full work history
              </Link>
            </p>
          )}
        </div>
      );
    }
    if (
      selection.kind === "position" &&
      position &&
      tab === "Overview" &&
      hrPosition &&
      organization &&
      inManagedCompany(selection)
    ) {
      const holders = context.people.filter((item) => item.positionId === position.id);
      const currentHolders = data.people.filter((item) => item.positionId === position.id);
      return (
        <div className={styles.stack}>
          <Facts
            values={[
              [
                "Level",
                context.directory.levels.find((item) => item.id === position.levelId)?.name ??
                  "Not assigned",
              ],
              [
                "Department",
                context.nodes.find((item) => item.id === position.departmentId)?.name ??
                  "Company-wide",
              ],
              [
                "Reports to",
                context.directory.positions.find((item) => item.id === position.reportsToPositionId)
                  ?.title ?? "Company leadership",
              ],
              ["People", `${holders.length} ${isArchived ? "archived" : "current"}`],
              ["Vacancy", currentHolders.length ? "Filled" : "Vacant"],
            ]}
          />
          <Button
            variant="secondary"
            type="button"
            onClick={() => setEditPosition((value) => !value)}
          >
            {editPosition ? "Close editor" : "Edit position"}
          </Button>
          {editPosition && (
            <PositionEditor
              key={hrPosition.id}
              base={base}
              data={organization}
              position={hrPosition}
            />
          )}
        </div>
      );
    }
    if (
      selection.kind === "position" &&
      position &&
      tab === "Compensation" &&
      hrPosition &&
      organization &&
      canReadPay
    ) {
      return <PositionDefaultPay base={base} data={organization} position={hrPosition} />;
    }
    if (selection.kind === "position" && tab === "History" && organization && local) {
      const impacted = organization.people.filter(
        (item) => item.position_id === selection.id && item.issues.length,
      );
      return (
        <div className={styles.stack}>
          <h3>Position changes and checks</h3>
          <p className={styles.muted}>
            Scheduled moves require a preview and preserve approved compensation.
          </p>
          {impacted.map((item) => (
            <p key={item.engagement_id} role="alert">
              <strong>{item.legal_name}</strong>: {item.issues.join(" · ")}
            </p>
          ))}
          {!impacted.length && <p>No reporting warnings for current holders.</p>}
        </div>
      );
    }
    if ((selection.kind === "department" || selection.kind === "team") && tab === "Overview") {
      const department = context.nodes.find((item) => item.id === selection.id);
      const branch = context.nodes.find((item) => item.id === department?.branchId);
      const head = authority?.seats.find(
        (item) => item.kind === "head" && item.scope_id === selection.id,
      );
      const flow = authority?.policies.find((item) => item.department_id === selection.id);
      return (
        <div className={styles.stack}>
          <Facts
            values={[
              ["Company", context.company?.name ?? "Not provided"],
              ["Branch", branch?.name ?? "Company-wide"],
              ["Department head", head?.person_name ?? "Not appointed"],
              [
                isArchived ? "Archived people" : "Current people",
                `${context.people.length} people`,
              ],
              ["Positions", `${context.positions.length} positions`],
              ...(local && canManage
                ? [
                    [
                      "Approval flow",
                      flow ? `${flow.version} published version` : "Not configured",
                    ] as [string, ReactNode],
                  ]
                : []),
            ]}
          />
          {!isArchived && local && canManage && organization && (
            <div className={styles.summary}>
              <strong>Reporting checks</strong>
              <span>
                {
                  organization.people.filter(
                    (item) => item.department_id === selection.id && item.issues.length,
                  ).length
                }{" "}
                people need review
              </span>
            </div>
          )}
        </div>
      );
    }
    if ((selection.kind === "company" || selection.kind === "branch") && tab === "Overview") {
      const branches = context.nodes.filter(
        (item) =>
          item.companyId === context.company?.id &&
          (item.kind === "branch" || item.kind === "headquarters"),
      );
      const currentBranches =
        selection.kind === "branch"
          ? branches.filter((item) => item.id === selection.id)
          : branches;
      const unassigned = context.people.filter((item) => !item.positionId);
      const vacant = context.positions.filter(
        (item) => !context.directory.people.some((person) => person.positionId === item.id),
      );
      const issues = local
        ? (organization?.people.filter((item) => item.issues.length).length ?? 0)
        : 0;
      const branchPeople = context.directory.people;
      return (
        <div className={styles.stack}>
          <Facts
            values={[
              ["Legal name", context.company?.name ?? "Not provided"],
              ["HR contact", "Not provided"],
              [
                isArchived ? "Archived people" : "Current people",
                `${context.people.length} people`,
              ],
              ["Positions", `${context.positions.length} positions`],
            ]}
          />
          {!isArchived && (
            <div className={styles.summary}>
              <strong>Needs attention</strong>
              <span>
                {unassigned.length} people without positions · {vacant.length} vacant positions
                {local && canManage ? ` · ${issues} reporting warnings` : ""}
              </span>
            </div>
          )}
          <h3>Branches</h3>
          <div className={styles.branchList}>
            {currentBranches.map((branch) => (
              <div key={branch.id}>
                <strong>{branch.name}</strong>
                <span>
                  {branchPeople.filter((item) => item.branchId === branch.id).length}{" "}
                  {isArchived ? "archived people" : "people"}
                </span>
                <span>
                  {context.directory.positions.filter((item) => item.branchId === branch.id).length}{" "}
                  positions
                </span>
              </div>
            ))}
          </div>
        </div>
      );
    }
    if (
      (selection.kind === "company" || selection.kind === "branch") &&
      tab === "Changes" &&
      organization &&
      inManagedCompany(selection)
    ) {
      return (
        <div className={styles.stack}>
          <OrganizationChangeQueue base={base} data={organization} />
        </div>
      );
    }
    if (
      (selection.kind === "company" || selection.kind === "branch") &&
      tab === "Settings" &&
      organization &&
      inManagedCompany(selection)
    ) {
      return (
        <div className={styles.stack}>
          <h3>Company settings</h3>
          {branding && (
            <details className={styles.section}>
              <summary>Branding</summary>
              <BrandAdministration {...branding} />
            </details>
          )}
          <details className={styles.section}>
            <summary>Branches, departments, and official documents</summary>
            <OrganizationUnits base={base} data={organization} />
          </details>
          <details className={styles.section}>
            <summary>Add position</summary>
            <PositionEditor base={base} data={organization} />
          </details>
          <OrganizationAdvancedTools base={base} data={organization} lifecycle={lifecycle} />
        </div>
      );
    }
    if (
      (selection.kind === "department" || selection.kind === "team") &&
      tab === "Approvals" &&
      authority &&
      organization &&
      inManagedCompany(selection)
    ) {
      const departmentId = selection.id;
      const filtered: Authority = {
        ...authority,
        departments: authority.departments.filter((item) => item.id === departmentId),
        seats: authority.seats.filter(
          (item) => item.scope_id === departmentId && item.kind === "head",
        ),
        policies: authority.policies.filter((item) => item.department_id === departmentId),
        canAppointKinds: authority.canAppointKinds.filter((item) => item === "head"),
        canPublishDepartmentIds: authority.canPublishDepartmentIds.filter(
          (item) => item === departmentId,
        ),
      };
      return (
        <DepartmentAuthorityWorkspace
          key={departmentId}
          holdingSlug={base.split("/")[2] ?? ""}
          initial={filtered}
          people={organization.people}
          positions={organization.positions}
        />
      );
    }
    return undefined;
  }

  return renderDetail(props.selection, props.tab, props.context) ?? null;
}

export function PeopleOrganizationWorkspace(props: PeopleOrganizationProps) {
  return props.accessOrganization && props.canManageStructure ? (
    <OrganizationDraftWorkspace
      key={`${props.actorId}:${props.currentCompanyId}`}
      base={props.base}
      companyName={
        props.data.companies.find((c) => c.id === props.currentCompanyId)?.name ?? "Company"
      }
      organization={props.accessOrganization}
      people={props}
    />
  ) : (
    <PeopleStructureWorkspace
      key={`${props.actorId}:${props.currentCompanyId}`}
      people={props}
      definition={readOnlyDefinition}
    />
  );
}
