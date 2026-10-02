import { drainOrganizationDrafts } from "./organization-drafts";
import "server-only";
import { iamDb } from "./iam-db";
import type { PostgresDatabase } from "./runtime/postgres";
import type { LifecycleCase } from "@/lib/people-lifecycle";
import { drainPositionMoves } from "./position-moves";

function caseDate(value:number,timezone:string){
  const parts=new Intl.DateTimeFormat("en-US",{timeZone:timezone,year:"numeric",month:"2-digit",day:"2-digit"}).formatToParts(new Date(value));
  const get=(part:string)=>parts.find(item=>item.type===part)?.value??"";
  return `${get("year")}-${get("month")}-${get("day")}`;
}

const placementFields = [
  "position_id", "branch_id", "department_id", "level_id",
  "manager_person_id", "job_title", "status", "start_date", "end_date",
] as const;
function placementEngagementChanged(engagement:Record<string,unknown>,snapshot:Record<string,unknown>|null){
  const captured=snapshot?.engagementPlacement;
  // Cases scheduled before placement fields were recorded still fail closed on version drift.
  if(!captured || typeof captured!=="object" || Array.isArray(captured) ||
    !placementFields.every(field=>Object.hasOwn(captured,field)))
    return Number(engagement.version)!==Number(snapshot?.engagementVersion);
  const values=captured as Record<string,unknown>;
  return placementFields.some(field=>(engagement[field]??null)!==(values[field]??null));
}

/** Idempotent due work. Authorization checks effective_at even while this worker is offline. */
export async function drainPeopleLifecycle(
  raw: PostgresDatabase,
  now = Date.now(),
) {
  const db = iamDb(raw),
    client = await db.pool.connect();
  let processed = 0;
  try {
    for (let count = 0; count < 50; count++) {
      await client.query("BEGIN ISOLATION LEVEL SERIALIZABLE");
      let currentId: string | undefined;
      try {
        // A Directory delivery uses the same lock while checking its placement
        // evidence. Defer this tick without failing the HR case if it is busy.
        const available = (await client.query("SELECT pg_try_advisory_xact_lock(821039320) AS available")).rows[0]?.available;
        if (!available) {
          await client.query("COMMIT");
          break;
        }
        const row = (
          await client.query(
            "SELECT * FROM people_lifecycle_cases WHERE status='scheduled' AND effective_at<=$1 AND next_attempt_at<=$1 ORDER BY effective_at,id FOR UPDATE SKIP LOCKED LIMIT 1",
            [now],
          )
        ).rows[0] as LifecycleCase | undefined;
        if (!row) {
          await client.query("COMMIT");
          break;
        }
        currentId = row.id;
        const outgoing = row.kind === "offboarding" || row.kind === "suspend";
        const people = (
          await client.query(
            "SELECT * FROM hr_people WHERE id=$1 AND company_id=$2 FOR UPDATE",
            [row.person_id, row.company_id],
          )
        ).rows[0];
        const engagement = (
          await client.query(
            "SELECT * FROM hr_engagements WHERE id=$1 AND person_id=$2 AND company_id=$3 FOR UPDATE",
            [row.engagement_id, row.person_id, row.company_id],
          )
        ).rows[0];
        if (!people || !engagement)
          throw Error("LIFECYCLE_REFERENCE_UNAVAILABLE");
        if (outgoing) {
          await client.query(
            "UPDATE assignments SET revoked_at=$3,version=version+1 WHERE company_id=$1 AND people_person_user($1,$2,user_id) AND revoked_at IS NULL",
            [row.company_id, row.person_id, row.effective_at],
          );
          await client.query(
            "UPDATE active_work_contexts SET assignment_id=NULL,assignment_version=NULL,revision=revision+1 WHERE company_id=$1 AND people_person_user($1,$2,user_id)",
            [row.company_id, row.person_id],
          );
          await client.query(
            "UPDATE hr_engagements SET status=$2,end_date=CASE WHEN $2='ended' THEN $3 ELSE end_date END,version=version+1,updated_at=$4 WHERE id=$1",
            [
              row.engagement_id,
              row.kind === "offboarding" ? "ended" : "suspended",
              row.definition.lastWorkingDate,
              now,
            ],
          );
          await client.query(
            "UPDATE employees SET status='inactive' WHERE company_id=$1 AND person_id=$2",
            [row.company_id, row.person_id],
          );
        } else {
          if (
            row.workflow !== "placement" &&
            !(
              await client.query("SELECT people_case_active($1,$2) ok", [
                row.id,
                now,
              ])
            ).rows[0].ok
          )
            throw Error("LIFECYCLE_REVIEW_CHANGED");
          let placementSnapshot:Record<string,unknown>|null=null;
          if (row.workflow === "placement") {
            const valid = (
              await client.query(
                "SELECT 1 FROM assignments a WHERE a.id=$1 AND iam_assignment_active(a.id,$2) AND iam_role_allows(a.role,a.company_id,'people.manage')",
                [
                  (row as LifecycleCase & { authorizing_assignment_id: string })
                    .authorizing_assignment_id,
                  now,
                ],
              )
            ).rowCount;
            if (!valid) throw Error("LIFECYCLE_AUTHORITY_CHANGED");
            const saved:unknown = (
              await client.query(
                "SELECT review_snapshot FROM people_lifecycle_cases WHERE id=$1",
                [row.id],
              )
            ).rows[0]?.review_snapshot;
            placementSnapshot=saved && typeof saved==="object" && !Array.isArray(saved)
              ? saved as Record<string,unknown> : null;
            if (placementEngagementChanged(engagement,placementSnapshot))
              throw Error("LIFECYCLE_PLACEMENT_CHANGED");
          }
          const position = (
            await client.query(
              "SELECT * FROM organization_positions WHERE company_id=$1 AND id=$2 AND active=1",
              [row.company_id, row.definition.positionId],
            )
          ).rows[0];
          if (
            !position ||
            engagement.status === "ended" ||
            people.status !== "active"
          )
            throw Error("LIFECYCLE_PLACEMENT_CHANGED");
          if (row.workflow === "placement") {
            if (
              position.level_id !== placementSnapshot?.levelId ||
              position.branch_id !== placementSnapshot?.positionBranch ||
              position.department_id !== placementSnapshot?.positionDepartment
            )
              throw Error("LIFECYCLE_PLACEMENT_CHANGED");
          }
          const manager = row.definition.managerPersonId;
          if (
            !position.is_ceo &&
            (!manager ||
              !(
                await client.query(
                  "SELECT 1 FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id JOIN staff_levels l ON l.id=e.level_id JOIN staff_levels target ON target.id=$3 WHERE p.id=$1 AND p.company_id=$2 AND p.status='active' AND e.status='active' AND l.level_number<target.level_number",
                  [manager, row.company_id, position.level_id],
                )
              ).rowCount)
          )
            throw Error("LIFECYCLE_MANAGER_CHANGED");
          if (row.workflow !== "placement") {
            // Only legacy reviewed changes replace previous legacy access. HR placement does not.
            await client.query(
              "UPDATE assignments SET revoked_at=$3,version=version+1 WHERE company_id=$1 AND source='lifecycle' AND source_id IN (SELECT g.id FROM people_lifecycle_grants g JOIN people_lifecycle_cases c ON c.id=g.case_id WHERE c.company_id=$1 AND c.person_id=$2 AND c.id<>$4 AND c.effective_at<=$3) AND revoked_at IS NULL",
              [row.company_id, row.person_id, row.effective_at, row.id],
            );
            // Rehire/restoration never revives grants that preceded the cutoff.
            await client.query(
              "UPDATE assignments SET revoked_at=$3,version=version+1 WHERE company_id=$1 AND people_person_user($1,$2,user_id) AND revoked_at IS NULL AND NOT EXISTS (SELECT 1 FROM people_lifecycle_grants g WHERE g.case_id=$4 AND g.assignment_id=assignments.id) AND EXISTS(SELECT 1 FROM people_access_cutoffs x WHERE x.company_id=$1 AND x.person_id=$2 AND x.effective_at<=$3 AND x.cancelled_at IS NULL AND x.restored_at IS NULL)",
              [row.company_id, row.person_id, row.effective_at, row.id],
            );
            await client.query(
              "UPDATE people_access_cutoffs SET restored_at=$3 WHERE company_id=$1 AND person_id=$2 AND cancelled_at IS NULL AND restored_at IS NULL AND effective_at<=$3",
              [row.company_id, row.person_id, row.effective_at],
            );
            await client.query(
              "UPDATE people_applied_packages SET ended_at=$3 WHERE company_id=$1 AND person_id=$2 AND source_kind='position' AND ended_at IS NULL",
              [row.company_id, row.person_id, row.effective_at],
            );
            await client.query(
              "INSERT INTO people_applied_packages(id,company_id,person_id,engagement_id,position_id,package_id,package_revision,definition,source_kind,scope,case_id,authorizing_assignment_id,valid_from,valid_until,created_at) SELECT 'legacy:'||c.id,c.company_id,c.person_id,c.engagement_id,c.definition->>'positionId',c.package_id,c.package_revision,r.definition,'position',c.review_snapshot,c.id,c.authorizing_assignment_id,c.effective_at,a.valid_until,$2 FROM people_lifecycle_cases c JOIN people_package_revisions r ON r.company_id=c.company_id AND r.package_id=c.package_id AND r.revision=c.package_revision JOIN assignments a ON a.id=c.authorizing_assignment_id WHERE c.id=$1 ON CONFLICT(id) DO NOTHING",
              [row.id, now],
            );
          }
          await client.query(
            "UPDATE people_lifecycle_cases SET status='completed' WHERE id=$1",
            [row.id],
          );
          await client.query("SELECT set_config('erpx.payroll_assignment_date',$1,true)",[caseDate(row.effective_at,row.definition.timezone)]);
          await client.query(
            "UPDATE hr_engagements SET position_id=$2,branch_id=$3,department_id=$4,level_id=$5,manager_person_id=$6,job_title=$7,status='active',version=version+1,updated_at=$8 WHERE id=$1",
            [
              row.engagement_id,
              position.id,
              position.branch_id ?? engagement.branch_id,
              position.department_id ?? engagement.department_id,
              position.level_id,
              manager,
              position.title,
              now,
            ],
          );
          if(row.definition.acceptRecommendedPay){
            const pay=(await client.query("SELECT pay_group_id,currency,monthly_amount_minor FROM position_pay_defaults WHERE company_id=$1 AND position_id=$2 ORDER BY version DESC LIMIT 1",[row.company_id,position.id])).rows[0];
            const existing=(await client.query("SELECT 1 FROM hr_compensation_revisions WHERE engagement_id=$1 LIMIT 1",[row.engagement_id])).rowCount;
            if(pay&&pay.currency===engagement.currency&&!existing){
              await client.query("UPDATE hr_engagements SET pay_group_id=$2 WHERE id=$1",[row.engagement_id,pay.pay_group_id]);
              await client.query("INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,pay_frequency,base_amount_minor,hourly_rate_minor,expected_hours,currency,components_json,status,created_by,created_at,updated_at) VALUES($1,$2,$3,'monthly',$4,0,0,$5,'[]','draft',$6,$7,$7) ON CONFLICT(id) DO NOTHING",[`comp-default-lifecycle-${row.id}`,row.engagement_id,caseDate(row.effective_at,row.definition.timezone),pay.monthly_amount_minor,pay.currency,row.created_by,now]);
            }
          }
          if(row.definition.acceptRecommendedLeave){
            const policy=(await client.query("SELECT d.policy_version_id,p.effective_from FROM level_leave_defaults d JOIN leave_policy_versions p ON p.id=d.policy_version_id WHERE d.company_id=$1 AND d.level_id=$2 ORDER BY d.version DESC LIMIT 1",[row.company_id,position.level_id])).rows[0];
            if(policy){
              const date=caseDate(row.effective_at,row.definition.timezone);
              const prior=(await client.query("SELECT a.id,a.effective_from,a.effective_to,p.policy_key FROM leave_policy_assignments a JOIN leave_policy_versions p ON p.id=a.policy_version_id WHERE a.company_id=$1 AND a.engagement_id=$2 AND (a.effective_to IS NULL OR a.effective_to>=$3) ORDER BY a.effective_from DESC LIMIT 1",[row.company_id,row.engagement_id,date])).rows[0];
              const managed=!prior||prior.policy_key==="legacy-untracked"||/^level-[1-6]-default$/.test(prior.policy_key);
              const changed=Boolean(engagement.level_id&&engagement.level_id!==position.level_id);
              if(managed&&(!prior||changed||prior.policy_key==="legacy-untracked")){
                const effective=changed?`${Number(date.slice(0,4))+1}-01-01`:date<policy.effective_from?policy.effective_from:date;
                if(prior&&prior.effective_from>=effective)throw Error("LIFECYCLE_LEAVE_ASSIGNMENT_OVERLAP");
                const until=new Date(Date.parse(`${effective}T00:00:00Z`)-86400000).toISOString().slice(0,10);
                if(prior&&prior.effective_from<=until&&(!prior.effective_to||prior.effective_to>until))await client.query("UPDATE leave_policy_assignments SET effective_to=$1,version=version+1 WHERE id=$2",[until,prior.id]);
                await client.query("INSERT INTO leave_policy_assignments(id,company_id,engagement_id,policy_version_id,effective_from,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(id) DO NOTHING",[`leave-default-lifecycle-${row.id}`,row.company_id,row.engagement_id,policy.policy_version_id,effective,row.created_by,now]);
              }
            }
          }
          await client.query(
            "UPDATE employees SET branch_id=$3,department_id=$4,status='active' WHERE company_id=$1 AND person_id=$2",
            [
              row.company_id,
              row.person_id,
              position.branch_id ?? engagement.branch_id,
              position.department_id ?? engagement.department_id,
            ],
          );
          await client.query(
            "INSERT INTO staff_organization_history(id,company_id,engagement_id,before_json,after_json,actor_id,reason,command_id,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
            [
              `history-${row.id}`,
              row.company_id,
              row.engagement_id,
              JSON.stringify({
                positionId: engagement.position_id,
                levelId: engagement.level_id,
                managerPersonId: engagement.manager_person_id,
              }),
              JSON.stringify({
                positionId: position.id,
                levelId: position.level_id,
                managerPersonId: manager,
              }),
              row.created_by,
              row.definition.reason,
              row.id,
              now,
            ],
          );
        }
        await client.query(
          "UPDATE people_lifecycle_cases SET status='completed',version=version+1,updated_at=$2 WHERE id=$1",
          [row.id, now],
        );
        await client.query(
          "INSERT INTO iam_audit VALUES($1,$2,'system:people-lifecycle','system:people-lifecycle','people.lifecycle.applied',$3,$4,$5)",
          [
            `applied-${row.id}`,
            row.company_id,
            row.id,
            JSON.stringify({
              effectiveAt: row.effective_at,
              kind: row.kind,
              personId: row.person_id,
            }),
            now,
          ],
        );
        await client.query(
          "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES($1,$2,'people.lifecycle.applied','people_lifecycle',$3,$4,$5,$6)",
          [
            `event-applied-${row.id}`,
            `system-apply-${row.id}`,
            row.id,
            row.version + 1,
            JSON.stringify({
              id: row.id,
              companyId: row.company_id,
              personId: row.person_id,
              kind: row.kind,
              state: "completed",
              effectiveAt: row.effective_at,
              version: row.version + 1,
            }),
            now,
          ],
        );
        await client.query("COMMIT");
        processed++;
      } catch (error) {
        await client.query("ROLLBACK");
        if (!currentId) throw error;
        // One failed request must not delay other departures. Keep its cutoff active.
        const code =
          error instanceof Error && /^LIFECYCLE_[A-Z_]+$/.test(error.message)
            ? error.message
            : ((error as { code?: string }).code ?? "LIFECYCLE_RETRY_REQUIRED");
        await client.query(
          "UPDATE people_lifecycle_cases SET attempts=attempts+1,next_attempt_at=$2+LEAST(3600000,1000*power(2,LEAST(attempts,12)))::bigint,last_error=$3 WHERE id=$1 AND status='scheduled'",
          [currentId, now, code],
        );
      }
    }
  } finally {
    client.release();
  }
  return processed + await drainPositionMoves(raw, now) + await drainOrganizationDrafts(raw, now);
}
