import { hasPermission } from "@/lib/permissions";
import Link from "next/link";
import { organizationOverview } from "@/server/organization";
import { MemberPanel } from "@/components/beta/member-panel";
import { PeopleEditor } from "./people-editor";
import { AccessEditor } from "./access-editor";
import { LevelAssignmentEditor } from "./level-assignment-editor";
import {
  canManagePeople,
  directoryOptions,
  personAccess,
} from "@/server/people-directory";
import { memberProfile } from "@/server/member-directory";
import { holdingResources } from "@/server/runtime";
import { staffSettings } from "@/server/staff-settings";
import type { WorkContext } from "@/server/authorization";
export async function MemberProfile({
  actor,
  base,
  member,
}: {
  actor: WorkContext;
  base: string;
  member: { id: string; name: string; email: string; personId: string | null };
}) {
  const person = await memberProfile(actor, member.personId),
    { write } = await holdingResources(actor.holdingId),
    options = await staffSettings(write, actor),
    manage = canManagePeople(actor),
    access = person ? await personAccess(actor, String(person.id)) : null;
  const history = person?.engagement_id
    ? (
        await write
          .prepare(
            "SELECT h.*,l.name level_name FROM staff_level_history h JOIN staff_levels l ON l.id=h.level_id WHERE h.engagement_id=? ORDER BY h.created_at DESC",
          )
          .bind(person.engagement_id)
          .all()
      ).results
    : [];
  return (
    <MemberPanel base={base} person={member} canRevoke={false}>
      {member.id===actor.userId&&<p><Link href={`${base}/people/payslips`}>My payslips</Link></p>}
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
          returnTo={`${base}/people?member=${encodeURIComponent(member.id)}`}
          nodes={await directoryOptions(actor)}
          levels={options.levels}
          people={options.people}
          organization={await organizationOverview(write,actor)}
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
          canManage={
            hasPermission(actor,"iam.manage") && member.id !== actor.userId
          }
          canRevokeCompany={
            hasPermission(actor,"iam.roles.manage") && member.id !== actor.userId
          }
        />
      )}
    </MemberPanel>
  );
}
