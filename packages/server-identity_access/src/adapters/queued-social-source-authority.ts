import 'server-only';
import {identityVerifiedContextProjections} from '@erpx/server-organization/public/identity-access-verified-context-projections';
import {identitySnapshotSources} from '@erpx/server-people_payroll/public/identity-snapshot-sources';
import {iamMemberVersionProbeSql} from './read-tail-family-r34';
import {hrSeatExistsSql} from './read-tail-family-r35';
import {revokedSessionHeadSql} from './read-tail-family-r36';

/** Fixed server-owned authorization row alias only. It validates the stored
 * original minimal StaffActor, never selects a replacement role/session. */
export function queuedSocialReplyAuthorityPredicate(){
 const organization=identityVerifiedContextProjections(),people=identitySnapshotSources();
 const row='pending',actor=`(CASE WHEN pg_input_is_valid(${row}.actor_json,'jsonb') THEN ${row}.actor_json::jsonb ELSE '{}'::jsonb END)`,iam=`(${actor}->'iam')`,context=`(${actor}->'authority')`,now='floor(extract(epoch FROM clock_timestamp())*1000)::bigint';
 const user=`(${actor}->>'userId')`,company=`${row}.company_id`,assignment=`(${actor}->>'assignmentId')`,role=`(${actor}->>'role')`,branch=`(${actor}->>'branchId')`,department=`(${actor}->>'departmentId')`,session=`(${context}->>'sessionId')`,number=(value:string)=>`(CASE WHEN ${value} ~ '^[0-9]{1,15}$' THEN (${value})::bigint END)`,version=number(`${iam}->>'queuedAssignmentVersion'`);
 return `${actor}->>'companyId'=${company} AND ${actor}->>'userId'=${row}.actor_id AND ${assignment} IS NOT NULL AND ${version}>0 AND EXISTS(SELECT 1 FROM assignments queued_assignment WHERE queued_assignment.id=${assignment} AND queued_assignment.user_id=${user} AND queued_assignment.role=${role} AND queued_assignment.company_id=${company} AND queued_assignment.version=${version} AND iam_assignment_active(queued_assignment.id,${now}) AND iam_principal_active(queued_assignment.user_id,queued_assignment.company_id,${now}) AND (queued_assignment.scope_kind='holding' OR queued_assignment.scope_kind='branch' AND queued_assignment.scope_node_id=${branch} OR queued_assignment.scope_kind='department' AND queued_assignment.scope_node_id=${department}) AND EXISTS(SELECT 1 FROM iam_roles queued_role WHERE queued_role.id=queued_assignment.role AND queued_role.company_id IN ('*',${company}) AND queued_role.status='active' AND (${iam}->>'roleRevision' IS NULL OR queued_role.published_revision=${number(`${iam}->>'roleRevision'`)})))
 AND NOT EXISTS(${revokedSessionHeadSql(`${session}`, `${company}`, `${user}`, 'aliased-session-first')})
 AND (${iam}->>'identityVersion' IS NULL OR COALESCE((${iamMemberVersionProbeSql('interp',company,user)}),0)=${number(`${iam}->>'identityVersion'`)})
 AND (${iam}->>'emergencyUntil' IS NULL OR ${number(`${iam}->>'emergencyUntil'`)}>${now})
 AND (${context} IS NULL OR ${context}='null'::jsonb OR EXISTS(SELECT 1 FROM active_work_contexts c WHERE c.session_id=${session} AND c.company_id=${company} AND c.user_id=${user} AND c.assignment_id=${assignment} AND c.revision=${number(`${context}->>'revision'`)} AND c.assignment_version=${number(`${context}->>'assignmentVersion'`)} AND c.assignment_version=${version} AND c.expires_at>${now}))
 AND (${iam}->>'hrAppointmentVersion' IS NULL OR EXISTS(${hrSeatExistsSql(organization.iam_store_authority_seats_s_from,people.iam_store_hr_people_p_join,people.iam_store_hr_engagements_e_join)} WHERE s.company_id=${company} AND s.kind='hr' AND s.scope_id=${company} AND s.version=${number(`${iam}->>'hrAppointmentVersion'`)} AND s.effective_at<=${now} AND iam_same_person(p.user_id,${user},s.company_id)))`;
}
