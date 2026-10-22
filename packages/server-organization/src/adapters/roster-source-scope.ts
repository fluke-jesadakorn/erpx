import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {platformOwnerProjectionSources} from '@erpx/server-people_payroll/public/platform-owner-projection-sources';
const people_payrollPlatformSources=platformOwnerProjectionSources;
/** The original Roster gap query serves both the owner workspace and the
 * notification predicate. The second mode has one closed source alias only. */
export function rosterCoverageGapSql(mode:'parameters'|'notification'='parameters'){
 const sql=`SELECT p.id person_id,p.legal_name,e.id engagement_id,?::date::text week_start
    FROM (${people_payrollPlatformSources.hr_people}) p JOIN (${people_payrollPlatformSources.hr_engagements}) e ON e.person_id=p.id AND e.company_id=p.company_id
    JOIN organization_positions pos ON pos.id=e.position_id AND pos.company_id=e.company_id AND pos.active=1 AND pos.roster_required=1
    WHERE p.company_id=? AND e.department_id=? AND p.status='active' AND p.merged_into_id IS NULL
      AND e.status='active' AND e.start_date::date<=(?::date+6) AND (e.end_date IS NULL OR e.end_date::date>=?::date)
      AND EXISTS (SELECT 1 FROM generate_series(0,6) AS day_offset(offset_days)
        CROSS JOIN LATERAL (SELECT ?::date + day_offset.offset_days AS calendar_day) day
        WHERE (extract(isodow FROM day.calendar_day)<=5 OR NOT EXISTS (
          SELECT 1 FROM generate_series(0,6) AS weekday_offset(offset_days)
          CROSS JOIN LATERAL (SELECT ?::date + weekday_offset.offset_days AS calendar_day) weekday
          WHERE extract(isodow FROM weekday.calendar_day)<=5 AND weekday.calendar_day>=e.start_date::date
            AND (e.end_date IS NULL OR weekday.calendar_day<=e.end_date::date)))
        AND day.calendar_day>=e.start_date::date AND (e.end_date IS NULL OR day.calendar_day<=e.end_date::date)
        AND NOT EXISTS (SELECT 1 FROM (${people_payrollPlatformSources.leave_requests}) l LEFT JOIN (${people_payrollPlatformSources.employees}) emp ON emp.id=l.employee_id AND emp.company_id=l.company_id
          WHERE l.company_id=e.company_id AND l.state='approved' AND coalesce(l.person_id,emp.person_id)=p.id
          AND (EXISTS (SELECT 1 FROM (${people_payrollPlatformSources.leave_day_segments}) ds WHERE ds.request_id=l.id AND ds.company_id=l.company_id AND ds.leave_date::date=day.calendar_day AND ds.segment='full')
            OR NOT EXISTS (SELECT 1 FROM (${people_payrollPlatformSources.leave_day_segments}) ds WHERE ds.request_id=l.id) AND day.calendar_day BETWEEN l.start_date::date AND coalesce(l.end_date::date,l.start_date::date+l.days::int-1)
              AND (day.calendar_day<>l.start_date::date OR l.start_segment='full') AND (l.end_date IS NULL OR day.calendar_day<>l.end_date::date OR l.end_segment='full'))))
      AND NOT EXISTS (SELECT 1 FROM (${people_payrollPlatformSources.people_roster_assignments}) a JOIN (${people_payrollPlatformSources.people_roster_shifts}) s ON s.id=a.shift_id AND s.state='active'
        JOIN (${people_payrollPlatformSources.people_roster_weeks}) w ON w.id=s.week_id WHERE w.company_id=e.company_id AND w.department_id=e.department_id AND w.week_start=?::date
        AND a.engagement_id=e.id AND a.state='assigned') ORDER BY p.legal_name,p.id`;
 if(mode==='parameters')return sql;
 const fields=['roster_source.week_start','roster_source.company_id','roster_source.department_id',...Array<string>(5).fill('roster_source.week_start')];let index=0;
 return sql.replace(/\?/g,()=>fields[index++]);
}
/** Original department/subtree selection, evaluated in the source statement. */
export function rosterNotificationDepartmentScope(actor:Pick<WorkContext,'companyId'|'branchId'|'departmentId'>){return {where:`EXISTS(WITH RECURSIVE roster_subtree AS(SELECT id FROM organization_nodes WHERE id=? AND company_id=? UNION ALL SELECT n.id FROM organization_nodes n JOIN roster_subtree parent ON n.parent_id=parent.id WHERE n.company_id=? AND n.status='active') SELECT 1 FROM organization_nodes d WHERE d.company_id=? AND d.kind='department' AND d.status='active' AND d.id=roster_source.department_id AND d.branch_id IS NOT DISTINCT FROM roster_source.branch_id AND (?::text IS NULL OR d.branch_id=?) AND (?::text IS NULL OR d.id IN(SELECT id FROM roster_subtree)))`,args:[actor.departmentId,actor.companyId,actor.companyId,actor.companyId,actor.branchId,actor.branchId,actor.departmentId]};}

/** Closed actual department source for notifications that precede a week. */
export function rosterNotificationDepartmentSource(){return "SELECT id,company_id,branch_id FROM organization_nodes WHERE kind='department' AND status='active'";}
