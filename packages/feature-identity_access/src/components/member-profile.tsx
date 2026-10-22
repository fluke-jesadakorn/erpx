import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { MemberPanel } from "@erpx/feature-collaboration/components/member-panel";
import { PeopleEditor } from "@erpx/feature-people_payroll/components/people-editor";
import { AccessEditor } from "@erpx/feature-identity_access/components/access-editor";
import { LevelAssignmentEditor } from "@erpx/feature-people_payroll/components/level-assignment-editor";
import { memberProfileDetails } from "@erpx/server-organization/public/member-profile-view";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
export async function MemberProfile({
  actor,
  base,
  member,
}: {
  actor: WorkContext;
  base: string;
  member: { id: string; name: string; email: string; personId: string | null };
}) {
  const { person, options, manage, access, history, nodes, organization, canManageAccess, canRevokeCompany } = await memberProfileDetails(actor, member);
  return (
    <MemberPanel base={base} person={member} canRevoke={false}>
      {member.id===actor.userId&&<p><Link href={canonicalUiHref(`${base}/people/payslips`)}>My payslips</Link></p>}
      {person ? (
        <dl>
          <dt>Department</dt>
          <dd>
            {options.departments.find((d) => d.id === person.department_id)
              ?.display_name ?? "Not set"}
          </dd>
          <dt>Staff level</dt>
          <dd>{String(person.level_name ?? "Not set")}</dd>
          <dt>Engagement</dt>
          <dd>{String(person.engagement_status ?? "Not set")}</dd>
          <dt>Reporting manager</dt>
          <dd>
            {options.people.find(
              (p) => p.person_id === person.manager_person_id,
            )?.legal_name ?? "Not set"}
          </dd>
        </dl>
      ) : (
        <p className="notice">
          An employee or contractor profile is needed before submitting
          expense, equipment, or purchase requests. An owner or HR administrator can complete it here.
        </p>
      )}
      {manage && (
        <PeopleEditor appearance="standard"
          base={base}
          person={person ?? undefined}
          member={!person?.user_id ? member : undefined}
          returnTo={canonicalUiHref(`${base}/people?member=${encodeURIComponent(member.id)}`)}
          nodes={nodes}
          levels={options.levels}
          people={options.people}
          organization={organization}
          levelManagedSeparately={options.canManage}
        />
      )}
      {options.canManage && person?.engagement_id && (
        <LevelAssignmentEditor
          base={base}
          engagementId={String(person.engagement_id)}
          version={Number(person.engagement_version)}
          levelId={person.level_id ? String(person.level_id) : null}
          levels={options.levels}
        />
      )}
      {history.length > 0 && (
        <details>
          <summary>Staff level history</summary>
          {history.map((h) => (
            <p key={String(h.id)}>
              {String(h.level_name)} · {String(h.reason)}
            </p>
          ))}
        </details>
      )}
      {access && person && (
        <AccessEditor appearance="standard"
          base={base}
          personId={String(person.id)}
          hasAccount={!!person.user_id}
          userId={String(person.user_id ?? "")}
          assignments={access.assignments}
          canManage={canManageAccess}
          canRevokeCompany={canRevokeCompany}
        />
      )}
    </MemberPanel>
  );
}
