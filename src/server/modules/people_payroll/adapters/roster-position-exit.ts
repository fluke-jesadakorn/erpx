import type {SqlDatabase,SqlStatement} from '../../runtime/public/runtime/types';import type {WorkContext} from '../../identity_access/public/authorization';import {rosterPositionExitNotifications} from '../../collaboration/public/people-roster-position-notifications';import {createRosterPositionExitApplication} from '../application/roster-position-exit';export function rosterPositionExitStatements(db:SqlDatabase,actor:WorkContext,engagementIds:string[],commandId:string,reason:string):SqlStatement[]{return createRosterPositionExitApplication({clock:{now:Date.now},stage(input){const {actor,engagementIds,commandId,reason}=input;const notificationStages=rosterPositionExitNotifications(db,input);  const ids = `{${engagementIds.join(",")}}`, now = input.now;
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
    notificationStages[0],
    db.prepare(`UPDATE people_roster_assignments SET state='released',version=version+1 WHERE id IN (SELECT id FROM (${affected}) a)`).bind(actor.companyId,ids,now),
    db.prepare(`INSERT INTO people_roster_history(id,company_id,department_id,request_id,record_id,action,actor_id,reason,evidence_json,created_at)
      SELECT ? || '-request-' || r.id,?,r.department_id,r.id,r.id,'request.invalidated.position',?,?,json_build_object('positionChange',true)::text,?
      FROM (${requests}) r ON CONFLICT DO NOTHING`).bind(commandId,actor.companyId,actor.userId,reason,now,actor.companyId,ids,ids,ids,ids),
    notificationStages[1],
    db.prepare(`UPDATE people_roster_requests SET state='invalidated',decision_note=?,version=version+1,updated_at=? WHERE id IN (SELECT id FROM (${requests}) r)`).bind(reason,now,actor.companyId,ids,ids,ids,ids),
  ];}}).rosterPositionExitStatements(actor,engagementIds,commandId,reason);}
