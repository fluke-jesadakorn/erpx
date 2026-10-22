import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {currentAssignmentReadScope} from '@erpx/server-identity_access/public/record-source-authority';
import {activeStaffSelect} from '@erpx/server-operations/public/staff-policy';
import {rosterCoverageGapSql,rosterNotificationDepartmentScope,rosterNotificationDepartmentSource} from '@erpx/server-organization/public/roster-source-scope';
import {purchasedCapabilityPredicate,packageClockSql} from '@erpx/server-commercial/public/purchased-capability-sql';
type Kind='week'|'request'|'coverage';
function source(actor:WorkContext,kind:Kind,id:string,company:string,coverageAsOf?:number){
 const department=rosterNotificationDepartmentScope(actor),current=currentAssignmentReadScope(actor,'workforce_actor_scope'),permissions=(['roster.read','roster.manage','roster.approve'] as const).map(permission=>currentAssignmentReadScope(actor,'workforce_actor_scope',permission));
 const manager=`(${permissions.map(permission=>`(${permission.where})`).join(' OR ')})`;
 // Original activeStaff selection and uniqueness are retained. Its validity
 // clocks use this statement; manager access does not require an engagement.
 let parameter=0;const staff=activeStaffSelect.replace(/\?/g,()=>parameter++===0?'?':'(SELECT now_ms FROM roster_source_clock)');
 const cte=`WITH roster_source_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms),roster_actor_candidates AS MATERIALIZED(${staff} AND p.user_id=?),roster_self AS(SELECT person_id,department_id FROM roster_actor_candidates WHERE (SELECT count(*) FROM roster_actor_candidates)=1)`;
 let from:string,policy:string,policyArgs:(string|number|null)[];
 if(kind==='coverage'){
  const manage=currentAssignmentReadScope(actor,'workforce_actor_scope','roster.manage');
  // pg_input_is_valid prevents a malformed notification date from reaching a
  // cast. Coverage may precede a week; the real department supplies its branch.
  from=`(SELECT parsed.parts[1] department_id,CASE WHEN pg_input_is_valid(parsed.parts[2],'date') THEN parsed.parts[2]::date::text END week_start,parsed.parts[3] person_id,${company} company_id FROM(SELECT regexp_match(${id},'^(.*):([0-9]{4}-[0-9]{2}-[0-9]{2}):([^:]+)$') parts)parsed) coverage_input JOIN(${rosterNotificationDepartmentSource()})roster_department ON roster_department.id=coverage_input.department_id AND roster_department.company_id=coverage_input.company_id CROSS JOIN LATERAL(SELECT coverage_input.*,roster_department.branch_id)roster_source`;
  const coverageClock=coverageAsOf===undefined?'to_timestamp((SELECT now_ms FROM roster_source_clock)::double precision/1000.0)':'to_timestamp(?::double precision/1000.0)',localWeek=`date_trunc('week',${coverageClock} AT TIME ZONE 'Asia/Bangkok')`;
  policy=`(${manage.where}) AND roster_source.week_start IN(to_char(${localWeek},'YYYY-MM-DD'),to_char(${localWeek}+interval '7 days','YYYY-MM-DD')) AND EXISTS(SELECT 1 FROM(${rosterCoverageGapSql('notification')})current_gap WHERE current_gap.person_id=roster_source.person_id)`;
  policyArgs=[...manage.args,...(coverageAsOf===undefined?[]:[coverageAsOf,coverageAsOf])];
 }else{
  from=kind==='week'?`people_roster_weeks roster_week CROSS JOIN LATERAL(SELECT roster_week.id,roster_week.company_id,roster_week.branch_id,roster_week.department_id,roster_week.state)roster_source`:`people_roster_requests roster_request JOIN people_roster_shifts roster_shift ON roster_shift.id=roster_request.target_shift_id JOIN people_roster_weeks roster_week ON roster_week.id=roster_shift.week_id AND roster_week.company_id=roster_request.company_id CROSS JOIN LATERAL(SELECT roster_request.id,roster_request.company_id,roster_week.branch_id,roster_week.department_id,roster_week.state)roster_source`;
  policy=kind==='week'?`(${manager} OR roster_source.state='published' AND EXISTS(SELECT 1 FROM roster_self WHERE roster_self.department_id=roster_source.department_id))`:`(${manager} OR EXISTS(SELECT 1 FROM roster_self WHERE roster_self.department_id=roster_source.department_id) AND EXISTS(SELECT 1 FROM hr_people participant WHERE participant.company_id=roster_source.company_id AND participant.id IN(roster_request.requester_person_id,roster_request.coworker_person_id) AND iam_same_person(participant.user_id,?,participant.company_id)))`;
  policyArgs=[...permissions.flatMap(permission=>permission.args),...(kind==='request'?[actor.userId]:[])];
 }
 return {sql:`${cte} SELECT 1 FROM ${from} CROSS JOIN roster_source_clock CROSS JOIN LATERAL(SELECT roster_source.company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ${kind==='coverage'?'TRUE':`roster_source.id=${id}`} AND roster_source.company_id=${company} AND (${current.where}) AND (${department.where}) AND (${policy}) AND ${purchasedCapabilityPredicate('roster_source.company_id','people.workforce','history','roster_source_clock.now_ms')}`,
  args:[actor.companyId,actor.userId,actor.branchId,actor.departmentId,...current.args,...department.args,...policyArgs]};
}
/** Both candidate kernels compose this before their original 100-row LIMIT. */
export function rosterNotificationReadScope(actor:WorkContext,surface:'staff'|'inbox',coverageAsOf?:number){
 const type=surface==='staff'?'staff_notifications.record_type':'e.source_type',id=surface==='staff'?'staff_notifications.record_id':'e.source_id',company=surface==='staff'?'staff_notifications.company_id':'e.company_id',queries=(['week','request','coverage'] as const).map(kind=>source(actor,kind,id,company,coverageAsOf));
 return {where:`(${type} NOT IN('roster_week','roster_request','roster_coverage') OR ${queries.map((query,index)=>`(${type}='roster_${(['week','request','coverage'] as const)[index]}' AND EXISTS(${query.sql}))`).join(' OR ')})`,args:queries.flatMap(query=>query.args)};
}
