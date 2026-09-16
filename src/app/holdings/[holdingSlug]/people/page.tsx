import { hasPermission } from "@/lib/permissions";
import { organizationOverview } from "@/server/organization";
import { BetaPeople } from "@/components/beta/people-page";
import { holdingResources } from "@/server/runtime";
import { staffSettings } from "@/server/staff-settings";
import { canManagePeople, directoryOptions } from "@/server/people-directory";
import { PeopleEditor } from "@/components/modules/people-editor";
import Link from "next/link";
import { PeopleNavigation } from "@/components/modules/people-navigation";
import { ArrowUpRight } from "lucide-react";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { PayrollRunEditor } from "@/components/modules/payroll-controls";
import {
  payrollOptions,
  listPeople,
  listPayrollRuns,
} from "@/server/people-payroll";
import {
  PageHeader,
  StatusBadge,
  EmptyState,
} from "@/components/ui/primitives";
import styles from "@/components/workplace/pages.module.css";
import { companyMembers } from "@/server/member-directory";
import { MemberProfile } from "@/components/modules/member-profile";

import directory from "./directory.module.css";

export const dynamic = "force-dynamic";

export default async function PeoplePage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{
    q?: string;
    status?: string;
    level?: string;
    department?: string;
    view?: string;
    member?: string;
    position?:string;
  }>;
}) {
  const { holdingSlug } = await params;
  const actor = await requireWorkContext(holdingSlug);
  const search = await searchParams;
  if (actor.beta) return <BetaPeople actor={actor} member={search.member} />;
  const [people, runs] = await Promise.all([
    listPeople(actor),
    listPayrollRuns(actor).catch(() => []),
  ]);
  const base = workspaceBase(publicScope(actor));
  const members = await companyMembers(actor),
    selectedMember = members.find((m) => m.id === search.member);

  const { write } = await holdingResources(actor.holdingId),
    staffOptions = await staffSettings(write, actor);
  const nodes = canManagePeople(actor) ? await directoryOptions(actor) : [];
  const organization = canManagePeople(actor) ? await organizationOverview(write,actor) : undefined;
  const filtered = people.filter(
    (p) =>
      (!search.level ||
        (search.level === "unset"
          ? !p.level_id
          : p.level_id === search.level)) &&
      (!search.department || p.department_id === search.department) &&
      (!search.q ||
        `${p.legal_name} ${p.email} ${p.job_title}`
          .toLowerCase()
          .includes(search.q.toLowerCase())) &&
      (search.status === "archived"
        ? p.status === "archived"
        : p.status !== "archived"),
  );
  const canPayroll =
    [
      "holding_owner",
      "hr_admin",
      "people_manager",
      "payroll_preparer",
      "payroll_approver",
      "auditor",
    ].includes(actor.role) &&
    !actor.branchId &&
    !actor.departmentId;
  const payroll = search.view === "payroll" && canPayroll;
  return (
    <main className="content" data-source="live">
      <PageHeader
        eyebrow="Your team"
        title="People"
        description="Find a teammate or manage their profile."
      />
      <PeopleNavigation base={base} actor={actor} scopeKind={actor.scope.kind} active={payroll ? "payroll" : "directory"} canPayroll={canPayroll}/>
      {!payroll && (
        <div className={directory.directory}>
          {selectedMember && (
            <MemberProfile actor={actor} base={base} member={selectedMember} />
          )}
          {members.some((m) => !m.personId) &&
            (canManagePeople(actor) ||
              members.some((m) => m.id === actor.userId && !m.personId)) && (
              <section className={`card ${directory.setup}`}>
                <h2>Member profile setup</h2>
                {members
                  .filter(
                    (m) =>
                      !m.personId &&
                      (canManagePeople(actor) || m.id === actor.userId),
                  )
                  .map((m) => (
                    <Link
                      className="row"
                      key={m.id}
                      href={`${base}/people?member=${encodeURIComponent(m.id)}`}
                    >
                      {m.name} · Complete employee profile
                    </Link>
                  ))}
              </section>
            )}
          {canManagePeople(actor) && (
            <PeopleEditor
              initialPositionId={organization?.positions.some(p=>p.id===search.position)?search.position:undefined}
              base={base}
              nodes={nodes}
              levels={staffOptions.levels}
              people={staffOptions.people}
              organization={organization}
            />
          )}
          <form className={`business-form card ${directory.search}`} action={`${base}/people`}>
            <label>
              Search people
              <input
                name="q"
                defaultValue={search.q}
                placeholder="Name, email or job title"
              />
            </label>
            <details className={directory.filters} open={Boolean(search.status === "archived" || search.level || search.department)}>
              <summary>More filters</summary>
              <div className={directory.filterFields}>
              <label>
                Directory status
                <select name="status" defaultValue={search.status ?? "current"}>
                  <option value="current">Current people</option>
                  <option value="archived">Archived people</option>
                </select>
              </label>
              <label>
                Staff level
                <select name="level" defaultValue={search.level ?? ""}>
                  <option value="">All levels</option>
                  <option value="unset">Not set</option>
                  {staffOptions.levels.map((l) => (
                    <option key={l.id} value={l.id}>
                      {l.name} · {l.level_number?`Level ${l.level_number}`:"Legacy level"}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                Department
                <select
                  name="department"
                  defaultValue={search.department ?? ""}
                >
                  <option value="">All departments</option>
                  {staffOptions.departments.map((d) => (
                    <option key={d.id} value={d.id}>
                      {d.display_name}
                    </option>
                  ))}
                </select>
              </label>
              </div>
            </details>
            <button className={`button secondary ${directory.searchButton}`} type="submit">
              Search people
            </button>
          </form>

          <div>
            <section className="card">
              <div className={styles.panelHead}>
                <h2>Directory</h2>
                <span className="muted">{filtered.length} matching people</span>
              </div>
              {filtered.length === 0 ? (
                <EmptyState
                  title="No matching people"
                  description="Change the search or status filter, or add a person to begin onboarding."
                />
              ) : (
                filtered.map((person) => (
                  <Link
                    className="row"
                    href={`${base}/people/${encodeURIComponent(String(person.id))}`}
                    key={String(person.id)}
                  >
                    <span className="avatar">
                      {String(person.legal_name)
                        .split(" ")
                        .map((part) => part[0])
                        .join("")
                        .slice(0, 2)}
                    </span>
                    <div className="row-main">
                      <div className="row-title">
                        {String(person.legal_name)}
                      </div>
                      <div className={styles.subline}>
                        {String(person.job_title ?? "No active engagement")} ·{" "}
                        {String(person.branch_name ?? person.country_code)}
                        <br />
                        {String(person.email ?? "No email")} ·{" "}
                        {String(person.level_name ?? "Not set")} ·{" "}
                        {person.user_id ? "Login linked" : "No login account"}
                      </div>
                    </div>
                    <StatusBadge
                      status={String(person.engagement_status ?? person.status)}
                    />
                  </Link>
                ))
              )}
            </section>
          </div>
        </div>
      )}
      {payroll && (
        <>
          <p className="notice">
            Gross-pay preview only. Statutory calculations and payment adapters
            are not yet available.
          </p>
          {hasPermission(actor,"payroll.prepare") &&
            !actor.branchId && (
              <PayrollRunEditor
                base={base}
                groups={await payrollOptions(actor)}
              />
            )}
          <section className="card" id="payroll-runs" style={{ marginTop: 24 }}>
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
              runs.slice(0, 10).map((run) => (
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
          </section>
        </>
      )}
    </main>
  );
}
