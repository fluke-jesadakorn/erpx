import { TimeText } from "@/components/time/time-preference-context";
import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { AccessEditor } from "@/components/modules/access-editor";
import { LevelAssignmentEditor } from "@/components/modules/level-assignment-editor";
import { RecordDocumentPanel } from "@/components/attachments/record-document-panel";
import {
CompensationEditor,
TimeEntryEditor,
} from "@/components/modules/payroll-controls";
import { PayrollCostAssignmentReview } from "@/components/modules/payroll-cost-assignments";
import { PeopleEditor } from "@/components/modules/people-editor";
import { LifecyclePanel } from "@/components/people/lifecycle-workspace";
import { StatusBadge } from "@/components/ui/primitives";
import { minorToDecimal } from "@/lib/finance-money";
import { samePerson } from "@/lib/iam";
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
import { RecordLink as Link } from "@/components/records/record-link";
import { forbidden, notFound } from "next/navigation";
import { Card } from "@/components/ui/foundation";

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
  if (!hasPermission(actor, "people.manage")) forbidden();
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
  const lifecycleData = canManagePeople(actor) && !!person.engagement_id ? await peopleLifecycleData(write,actor) : null;
  const employeeSelf =
      samePerson(actor, person.user_id) || actor.iam?.personId === String(person.id),
    canReadEmployeeDocuments = hasPermission(actor, "people.documents.read"),
    canManageEmployeeDocuments = hasPermission(actor, "people.documents.manage"),
    employeeDocumentPurposes = canReadEmployeeDocuments
      ? [
          { key: "identity_document", label: "Identity document", description: "Attach the controlled identity document used for employment administration." },
          { key: "work_permit", label: "Work permit", description: "Attach the current work permit or authorization." },
          { key: "tax_document", label: "Tax document", description: "Attach employee tax registration or declaration documents." },
          { key: "bank_document", label: "Bank document", description: "Attach verified payroll bank-account evidence." },
          { key: "employment_contract", label: "Employment contract", description: "Attach the signed employment contract.", editable: canManageEmployeeDocuments },
          { key: "employment_amendment", label: "Contract amendment", description: "Attach a signed contract amendment or role change document.", editable: canManageEmployeeDocuments },
          { key: "professional_certificate", label: "Professional certificate", description: "Attach a professional, trade, or eligibility certificate." },
          { key: "compensation_authorization", label: "Compensation authorization", description: "Attach the restricted approval supporting a compensation decision." },
        ]
      : employeeSelf
        ? [
            { key: "employment_contract", label: "Employment contract", description: "Your self-visible signed employment contract.", editable: false },
            { key: "employment_amendment", label: "Contract amendment", description: "Your self-visible signed contract amendments.", editable: false },
            { key: "professional_certificate", label: "Professional certificate", description: "Attach or review your professional certificate.", editable: true },
          ]
        : [];
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
      <div className="ui-section-header">
        <div>
          <h1>{String(person.legal_name)}</h1>
          <p className="muted">
            {String(person.job_title ?? "")} · {String(person.email ?? "")}
          </p>
        </div>
        <StatusBadge appearance="standard"
          status={String(person.engagement_status ?? person.status)}
        />
      </div>

      <FeatureTabs/><WidgetGrid>
      {lifecycleData && <FeatureWidget appearance="standard" id="person-lifecycle" title="Lifecycle"><LifecyclePanel base={base} data={lifecycleData} person={{id,name:String(person.legal_name),status:String(person.engagement_status),engagementId:String(person.engagement_id),positionId:person.position_id?String(person.position_id):null,managerPersonId:person.manager_person_id?String(person.manager_person_id):null}}/></FeatureWidget>}
      {true && <FeatureWidget appearance="standard" id="person-profile" title="Profile"><>
          <Card as="section" appearance="standard" className="ui-panel">
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
            {data.recommendedLeave&&<p>Level leave recommendation: {data.recommendedLeave.annual_days} annual days and {data.recommendedLeave.sick_days} sick days / year · policy v{data.recommendedLeave.version}</p>}
            {data.leaveAssignment&&<p>Assigned leave policy: {data.leaveAssignment.name} · v{data.leaveAssignment.version} · effective {data.leaveAssignment.effective_from}</p>}
          </Card>
          {employeeDocumentPurposes.length > 0 && (
            <RecordDocumentPanel appearance="standard"
              base={base}
              targetType="hr_person"
              targetId={String(person.id)}
              expectedTargetVersion={Number(person.version)}
              editable={canManageEmployeeDocuments || employeeSelf}
              title="Employee documents"
              purposes={employeeDocumentPurposes}
            />
          )}
          {canReadEmployeeDocuments && lifecycleData?.cases
            .filter((item) => item.person_id === id)
            .map((item) => {
              const purpose = item.kind === "onboarding"
                ? { key: "offer_document", label: "Offer and onboarding document", description: "Attach the reviewed offer or onboarding document for this lifecycle case." }
                : item.kind === "transfer"
                  ? { key: "transfer_document", label: "Transfer document", description: "Attach the approved transfer or placement document for this case." }
                  : { key: "offboarding_document", label: "Offboarding document", description: "Attach the approved offboarding, suspension, or exit document for this case." };
              return (
                <RecordDocumentPanel appearance="standard"
                  key={item.id}
                  base={base}
                  targetType="people_lifecycle_case"
                  targetId={item.id}
                  expectedTargetVersion={Number(item.version)}
                  editable={canManageEmployeeDocuments}
                  title={`${item.kind} case documents`}
                  purposes={[purpose]}
                />
              );
            })}
          {canManagePeople(actor) && (
            <PeopleEditor appearance="standard"
              lifecycleManaged={!actor.beta}
              base={base}
              person={person}
              nodes={nodes}
              levels={staffOptions.levels}
              people={staffOptions.people}
              organization={await organizationOverview(write,actor)}
              gpoPositions={lifecycleData?.positions}
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
          <details className="ui-panel">
            <summary>Level history</summary>
            {!levelHistory.length && <p>No level changes recorded.</p>}
            {levelHistory.map((h) => (
              <p key={String(h.id)}>
                {String(h.previous_level_name ?? "Not set")} →{" "}
                {String(h.level_name)} · {String(h.reason)} ·{" "}
                {<TimeText appearance="standard" value={Number(h.created_at)} />}
              </p>
            ))}
          </details>
        </></FeatureWidget>}
      {access && <FeatureWidget appearance="standard" id="person-access" title="Login & access">
        <AccessEditor appearance="standard"
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
      {true && <FeatureWidget appearance="standard" id="person-work" title="Work history"><Card as="section" appearance="standard" className="ui-panel">
          <h2>Leave history</h2>
          {!data.leave.length ? (
            <p>No leave requests linked to this worker.</p>
          ) : (
            data.leave.map((item) => (
              <div className="ui-record-item" key={String(item.id)}>
                <div className="row-main">
                  <strong>{String(item.number)}</strong>
                  <p>
                    {String(item.start_date)} · {String(item.days)} days
                  </p>
                </div>
                <StatusBadge appearance="standard" status={String(item.state)} />
              </div>
            ))
          )}
        </Card></FeatureWidget>}
      {data.privateAccess && Boolean(person.engagement_id) && <FeatureWidget appearance="standard" id="person-pay" title="Pay & time">
          <Card as="section" appearance="standard" className="ui-panel">
            
            <p>
              Approved current base pay:{" "}
              {person.base_amount_minor == null
                ? "Not set"
                : `${person.currency} ${minorToDecimal(Number(person.base_amount_minor))}`}
            </p>
            {data.recommendedPay&&<p>Position recommendation: {data.recommendedPay.currency} {minorToDecimal(data.recommendedPay.monthly_amount_minor)} / month · v{data.recommendedPay.version}</p>}
            {data.revisions.filter((revision:Record<string,unknown>)=>revision.status==="draft").map((revision:Record<string,unknown>)=><p key={String(revision.id)}>Pay draft awaiting another approver: {String(revision.currency)} {minorToDecimal(Number(revision.base_amount_minor))} · effective {String(revision.effective_from)}</p>)}
          </Card>
          <CompensationEditor appearance="standard"
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
          {hasPermission(actor,"payroll.prepare")&&!actor.branchId&&!actor.departmentId&&<PayrollCostAssignmentReview base={base} engagementId={String(person.engagement_id)} userId={actor.userId}/>}
          <TimeEntryEditor appearance="standard"
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
