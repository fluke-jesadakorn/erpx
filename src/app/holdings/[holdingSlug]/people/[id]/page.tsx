import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { AccessEditor } from "@/components/modules/access-editor";
import { LevelAssignmentEditor } from "@/components/modules/level-assignment-editor";
import {
CompensationEditor,
TimeEntryEditor,
} from "@/components/modules/payroll-controls";
import { PeopleEditor } from "@/components/modules/people-editor";
import { LifecyclePanel } from "@/components/people/lifecycle-workspace";
import { StatusBadge } from "@/components/ui/primitives";
import { minorToDecimal } from "@/lib/finance-money";
import { hasPermission } from "@/lib/permissions";
import { workspaceBase } from "@/lib/workspace";
import { publicScope,requireWorkContext } from "@/server/authorization";
import { organizationOverview } from "@/server/organization";
import { accessPage,pageError } from "@/server/page-error";
import {
canManagePeople,
directoryOptions,
personAccess,
} from "@/server/people-directory";
import { peopleLifecycleData } from "@/server/people-lifecycle";
import { getPerson,payrollOptions } from "@/server/people-payroll";
import { holdingResources } from "@/server/runtime";
import { companyAuthority } from "@/server/staff-policy";
import { staffSettings } from "@/server/staff-settings";
import { ArrowLeft } from "lucide-react";
import Link from "next/link";
import { notFound } from "next/navigation";

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
  } catch (error) {
    return pageError(error, holdingSlug);
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
  if (!tabs.some(([id]) => id === tab)) {
    if (["pay", "access"].includes(tab)) return accessPage("permission", holdingSlug);
    notFound();
  }
  return (
    <FeaturePage className="content-stack" data-source="live">
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

      <FeatureTabs/><WidgetGrid>
      {canManagePeople(actor) && !!person.engagement_id && <FeatureWidget id="person-lifecycle" title="Lifecycle"><LifecyclePanel base={base} data={await peopleLifecycleData(write,actor)} person={{id,name:String(person.legal_name),status:String(person.engagement_status),engagementId:String(person.engagement_id),positionId:person.position_id?String(person.position_id):null,managerPersonId:person.manager_person_id?String(person.manager_person_id):null}}/></FeatureWidget>}
      {true && <FeatureWidget id="person-profile" title="Profile"><>
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
              lifecycleManaged={!actor.beta}
              base={base}
              person={person}
              nodes={nodes}
              levels={staffOptions.levels}
              people={staffOptions.people}
              organization={await organizationOverview(write,actor)}
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
        </></FeatureWidget>}
      {access && <FeatureWidget id="person-access" title="Login & access">
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
            hasPermission(actor,"iam.manage") && person.user_id !== actor.userId
          }
        />
      </FeatureWidget>}
      {true && <FeatureWidget id="person-work" title="Work history"><section className="card">
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
        </section></FeatureWidget>}
      {data.privateAccess && Boolean(person.engagement_id) && <FeatureWidget id="person-pay" title="Pay & time">
          <section className="card">
            
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
        </FeatureWidget>}
    </WidgetGrid></FeaturePage>
  );
}
