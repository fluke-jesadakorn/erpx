import {collaborationNotificationSnapshotSources as peoplepayrollSnapshotSources} from '../../people_payroll/public/collaboration-notification-snapshot-sources';
const peoplepayrollSources=peoplepayrollSnapshotSources();
import type { SqlDatabase } from '../../runtime/public/runtime/types';
import type { RosterPositionExit } from '../../people_payroll/public/roster-position-exit-contract';
export function rosterPositionExitNotifications(db: SqlDatabase, input: RosterPositionExit) {
    const { actor, engagementIds } = input;
    const ids = `{${engagementIds.join(",")}}`, now = input.now;
    const affected = `SELECT a.id,a.person_id,a.engagement_id,s.week_id,w.department_id
    ${peoplepayrollSources.people_roster_position_notifications_people_roster_assignments_a_from} ${peoplepayrollSources.people_roster_position_notifications_people_roster_shifts_s_join} ON s.id=a.shift_id
    ${peoplepayrollSources.people_roster_position_notifications_people_roster_weeks_w_join} ON w.id=s.week_id
    WHERE w.company_id=? AND a.engagement_id=ANY(?::text[]) AND a.state='assigned' AND s.state='active' AND s.starts_at>?`;
    const requests = `SELECT r.* ${peoplepayrollSources.people_roster_position_notifications_people_roster_requests_r_from} WHERE r.company_id=? AND r.state IN ('pending_coworker','pending_manager')
    AND (r.requester_person_id IN (SELECT person_id ${peoplepayrollSources.people_roster_position_notifications_hr_engagements_from} WHERE id=ANY(?::text[]))
      OR r.coworker_person_id IN (SELECT person_id ${peoplepayrollSources.people_roster_position_notifications_hr_engagements_from} WHERE id=ANY(?::text[]))
      OR r.source_assignment_id IN (SELECT id ${peoplepayrollSources.people_roster_position_notifications_people_roster_assignments_from} WHERE engagement_id=ANY(?::text[]))
      OR r.target_assignment_id IN (SELECT id ${peoplepayrollSources.people_roster_position_notifications_people_roster_assignments_from} WHERE engagement_id=ANY(?::text[])))`;
    return [db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
      SELECT gen_random_uuid()::text,?,p.user_id,'roster_week',a.week_id,w.version,'A future shift was removed after a Position change.',?
      FROM (${affected}) a ${peoplepayrollSources.people_roster_position_notifications_hr_people_p_join} ON p.id=a.person_id AND p.company_id=? ${peoplepayrollSources.people_roster_position_notifications_people_roster_weeks_w_join} ON w.id=a.week_id
      WHERE p.user_id IS NOT NULL ON CONFLICT DO NOTHING`).bind(actor.companyId, now, actor.companyId, ids, now, actor.companyId),
        db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
      SELECT gen_random_uuid()::text,?,p.user_id,'roster_request',r.id,r.version+1,'A pending roster request closed after a Position change.',?
      FROM (${requests}) r ${peoplepayrollSources.people_roster_position_notifications_hr_people_p_join} ON p.id IN (r.requester_person_id,r.coworker_person_id) AND p.company_id=?
      WHERE p.user_id IS NOT NULL ON CONFLICT DO NOTHING`).bind(actor.companyId, now, actor.companyId, ids, ids, ids, ids, actor.companyId)];
}
