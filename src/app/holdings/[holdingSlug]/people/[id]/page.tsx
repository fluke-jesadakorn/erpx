import { hasBusinessRole } from "@/lib/responsibilities";
import { companyAuthority } from "@/server/staff-policy";
import { LevelAssignmentEditor } from "@/components/modules/level-assignment-editor";
import styles from "@/components/workplace/pages.module.css";
import { holdingResources } from "@/server/runtime";
import { staffSettings } from "@/server/staff-settings";
import { AccessEditor } from "@/components/modules/access-editor";
import {
  canManagePeople,
  directoryOptions,
  personAccess,
} from "@/server/people-directory";
import { PeopleEditor } from "@/components/modules/people-editor";
import Link from "next/link";
import { notFound } from "next/navigation";
import { ArrowLeft } from "lucide-react";
import { publicScope, requireWorkContext } from "@/server/authorization";
import {
  CompensationEditor,
  TimeEntryEditor,
} from "@/components/modules/payroll-controls";
import { getPerson, payrollOptions } from "@/server/people-payroll";
import { workspaceBase } from "@/lib/workspace";
import { minorToDecimal } from "@/lib/finance-money";
import { StatusBadge } from "@/components/ui/primitives";

export const dynamic = "force-dynamic";

export default async function PersonDetail({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
  searchParams: Promise<{ tab?: string }>;
}) {
  const { holdingSlug, id } = await params;
  const actor = await requireWorkContext(holdingSlug);
  let data;
  try {
    data = await getPerson(actor, id);
  } catch {
    notFound();
  }
  const base = workspaceBase(publicScope(actor));
  const person = data.person;
  const [nodes, access] = await Promise.all([
    canManagePeople(actor) ? directoryOptions(actor) : Promise.resolve([]),
    personAccess(actor, id),
  ]);
  const { write } = await holdingResources(actor.holdingId),
    staffOptions = await staffSettings(write, actor),
    levelHistory = (
      await write
        .prepare(
          "SELECT h.*,l.name level_name,old.name previous_level_name FROM staff_level_history h JOIN staff_levels l ON l.id=h.level_id LEFT JOIN staff_levels old ON old.id=h.previous_level_id WHERE h.engagement_id=? ORDER BY h.created_at DESC",
        )
        .bind(person.engagement_id ?? null)
        .all()
    ).results;
  const groups = await payrollOptions(actor).catch(() => []);
  const query = await searchParams;
  const tabs = [
    ["profile", "Profile"],
    ["work", "Work history"],
    ...(data.privateAccess ? [["pay", "Pay & time"]] : []),
    ...(access ? [["access", "Login & access"]] : []),
  ];
  const tab = query.tab ?? "profile";
  if (!tabs.some(([id]) => id === tab)) notFound();
  return (
    <main className="content-stack" data-source="live">
      <Link className="back-link" href={`${base}/people`}>
        <ArrowLeft size={14} /> Back to people
      </Link>
      <div className="section-head">
        <div>
          <h1>{String(person.legal_name)}</h1>
          <p className="muted">
            {String(person.job_title ?? "")} · {String(person.email ?? "")}
          </p>
        </div>
        <StatusBadge
          status={String(person.engagement_status ?? person.status)}
        />
      </div>
      <nav className={styles.tabs} aria-label="Person sections">
        {tabs.map(([key, label]) => (
          <Link
            key={key}
            href={`${base}/people/${id}?tab=${key}`}
            aria-current={tab === key ? "page" : undefined}
          >
            {label}
          </Link>
        ))}
      </nav>
      {tab === "profile" && (
        <>
          <section className="card">
            <dl className="staff-facts">
              {[
                ["Staff level", person.level_name ?? "Not set"],
                ["Worker type", person.worker_type],
                ["Start date", person.start_date],
                ["Currency", person.currency],
              ].map(([label, value]) => (
                <div key={String(label)}>
                  <dt>{String(label)}</dt>
                  <dd>{String(value ?? "—")}</dd>
                </div>
              ))}
            </dl>
          </section>
          {canManagePeople(actor) && (
            <PeopleEditor
              base={base}
              person={person}
              nodes={nodes}
              levels={staffOptions.levels}
              people={staffOptions.people}
              levelManagedSeparately={staffOptions.canManage}
            />
          )}
          {staffOptions.canManage && Boolean(person.engagement_id) && (
            <LevelAssignmentEditor
              base={base}
              engagementId={String(person.engagement_id)}
              version={Number(person.engagement_version)}
              levelId={person.level_id ? String(person.level_id) : null}
              levels={staffOptions.levels}
            />
          )}
          <details className="card">
            <summary>Level history</summary>
            {!levelHistory.length && <p>No level changes recorded.</p>}
            {levelHistory.map((h) => (
              <p key={String(h.id)}>
                {String(h.previous_level_name ?? "Not set")} →{" "}
                {String(h.level_name)} · {String(h.reason)} ·{" "}
                {new Date(Number(h.created_at)).toLocaleDateString("en-GB")}
              </p>
            ))}
          </details>
        </>
      )}
      {tab === "access" && access && (
        <AccessEditor
          base={base}
          personId={id}
          hasAccount={Boolean(person.user_id)}
          userId={person.user_id ? String(person.user_id) : undefined}
          canRevokeCompany={
            person.user_id !== actor.userId &&
            (await companyAuthority(write, actor, ["holding_owner"]))
          }
          assignments={access.assignments}
          canManage={
            hasBusinessRole(actor, ["holding_owner"]) && person.user_id !== actor.userId
          }
        />
      )}
      {tab === "work" && (
        <section className="card">
          <h2>Leave history</h2>
          {!data.leave.length ? (
            <p>No leave requests linked to this worker.</p>
          ) : (
            data.leave.map((item) => (
              <div className="row" key={String(item.id)}>
                <div className="row-main">
                  <strong>{String(item.number)}</strong>
                  <p>
                    {String(item.start_date)} · {String(item.days)} days
                  </p>
                </div>
                <StatusBadge status={String(item.state)} />
              </div>
            ))
          )}
        </section>
      )}
      {tab === "pay" && data.privateAccess && Boolean(person.engagement_id) && (
        <>
          <section className="card">
            <h2>Pay & time</h2>
            <p>
              Base pay:{" "}
              {person.base_amount_minor == null
                ? "Not set"
                : `${person.currency} ${minorToDecimal(Number(person.base_amount_minor))}`}
            </p>
          </section>
          <CompensationEditor
            base={base}
            person={person}
            groups={groups}
            revisions={data.revisions}
            canEdit={canManagePeople(actor)}
            canApprove={[
              "holding_owner",
              "hr_admin",
              "payroll_approver",
            ].includes(actor.role)}
            userId={actor.userId}
          />
          <TimeEntryEditor
            base={base}
            person={person}
            entries={data.time}
            canEdit={canManagePeople(actor) || person.user_id === actor.userId}
            canApprove={[
              "holding_owner",
              "people_manager",
              "hr_admin",
              "payroll_preparer",
            ].includes(actor.role)}
            userId={actor.userId}
          />
        </>
      )}
    </main>
  );
}
