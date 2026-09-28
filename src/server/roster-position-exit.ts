import type { SqlDatabase, SqlStatement } from "./runtime/types";
import type { WorkContext } from "./authorization";

/** Statements run in the same command batch as the HR placement change. */
export function rosterPositionExitStatements(db: SqlDatabase, actor: WorkContext, engagementIds: string[], commandId: string, reason: string): SqlStatement[] {
  if (!engagementIds.length) return [];
  const ids = `{${engagementIds.join(",")}}`, now = Date.now();
  const affected = `SELECT a.id,a.person_id,a.engagement_id,s.week_id,w.department_id
    FROM people_roster_assignments a JOIN people_roster_shifts s ON s.id=a.shift_id
    JOIN people_roster_weeks w ON w.id=s.week_id
    WHERE w.company_id=? AND a.engagement_id=ANY(?::text[]) AND a.state='assigned' AND s.state='active' AND s.starts_at>?`;
  const requests = `SELECT r.* FROM people_roster_requests r WHERE r.company_id=? AND r.state IN ('pending_coworker','pending_manager')
    AND (r.requester_person_id IN (SELECT person_id FROM hr_engagements WHERE id=ANY(?::text[]))
      OR r.coworker_person_id IN (SELECT person_id FROM hr_engagements WHERE id=ANY(?::text[]))
      OR r.source_assignment_id IN (SELECT id FROM people_roster_assignments WHERE engagement_id=ANY(?::text[]))
      OR r.target_assignment_id IN (SELECT id FROM people_roster_assignments WHERE engagement_id=ANY(?::text[])))`;
  return [
    db.prepare(`UPDATE people_roster_locks SET version=version+1 WHERE company_id=?`).bind(actor.companyId),
    db.prepare(`UPDATE people_roster_weeks SET version=version+1,updated_at=? WHERE id IN (SELECT week_id FROM (${affected}) affected)`).bind(now,actor.companyId,ids,now),
    db.prepare(`INSERT INTO people_roster_history(id,company_id,department_id,week_id,record_id,action,actor_id,reason,evidence_json,created_at)
      SELECT ? || '-release-' || a.id,?,a.department_id,a.week_id,a.id,'assignment.released.position',?,?,json_build_object('engagementId',a.engagement_id)::text,?
      FROM (${affected}) a ON CONFLICT DO NOTHING`).bind(commandId,actor.companyId,actor.userId,reason,now,actor.companyId,ids,now),
    db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
      SELECT gen_random_uuid()::text,?,p.user_id,'roster_week',a.week_id,w.version,'A future shift was removed after a Position change.',?
      FROM (${affected}) a JOIN hr_people p ON p.id=a.person_id AND p.company_id=? JOIN people_roster_weeks w ON w.id=a.week_id
      WHERE p.user_id IS NOT NULL ON CONFLICT DO NOTHING`).bind(actor.companyId,now,actor.companyId,ids,now,actor.companyId),
    db.prepare(`UPDATE people_roster_assignments SET state='released',version=version+1 WHERE id IN (SELECT id FROM (${affected}) a)`).bind(actor.companyId,ids,now),
    db.prepare(`INSERT INTO people_roster_history(id,company_id,department_id,request_id,record_id,action,actor_id,reason,evidence_json,created_at)
      SELECT ? || '-request-' || r.id,?,r.department_id,r.id,r.id,'request.invalidated.position',?,?,json_build_object('positionChange',true)::text,?
      FROM (${requests}) r ON CONFLICT DO NOTHING`).bind(commandId,actor.companyId,actor.userId,reason,now,actor.companyId,ids,ids,ids,ids),
    db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
      SELECT gen_random_uuid()::text,?,p.user_id,'roster_request',r.id,r.version+1,'A pending roster request closed after a Position change.',?
      FROM (${requests}) r JOIN hr_people p ON p.id IN (r.requester_person_id,r.coworker_person_id) AND p.company_id=?
      WHERE p.user_id IS NOT NULL ON CONFLICT DO NOTHING`).bind(actor.companyId,now,actor.companyId,ids,ids,ids,ids,actor.companyId),
    db.prepare(`UPDATE people_roster_requests SET state='invalidated',decision_note=?,version=version+1,updated_at=? WHERE id IN (SELECT id FROM (${requests}) r)`).bind(reason,now,actor.companyId,ids,ids,ids,ids),
  ];
}
