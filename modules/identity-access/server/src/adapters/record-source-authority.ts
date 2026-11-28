export function assignmentReadSql(alias: string, permission: string) {
    return `EXISTS(SELECT 1 FROM assignments g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=${alias}.company_id
    AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND iam_role_allows(g.role,g.company_id,'${permission}')
    AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=${alias}.branch_id OR g.scope_kind='department' AND g.scope_node_id=${alias}.department_id))`;
}

import type {StaffActor} from '@erpx/operations-server/public/staff-policy';
import {hasPermission} from '@erpx/identity-access-ui/lib/permissions';
import {revokedSessionProbeSql} from './iam-probe-sql';
import {iamMemberVersionProbeSql} from './read-tail-family-r34';
import {hrSeatExistsSql} from './read-tail-family-r35';
import {identityVerifiedContextProjections} from '@erpx/organization-server/public/identity-access-verified-context-projections';
import {identitySnapshotSources} from '@erpx/people-payroll-server/public/identity-snapshot-sources';
const organization=identityVerifiedContextProjections(),people=identitySnapshotSources();
type WorkforceSourcePermission='leave.evidence.read'|'people.time.manage'|'people.compensation.read'|'people.compensation.payroll'|'people.manage'|'roster.read'|'roster.manage'|'roster.approve';
/** The alias is a closed owner scope, never caller-supplied SQL. This is the
 * read equivalent of current Work-context and IAM command guards, in the same
 * statement as the source and LIMIT. Permission-free employee access still
 * requires the exact current principal, assignment and selected scope. */
export function currentAssignmentReadScope(actor:StaffActor,alias:'workforce_actor_scope',permission?:WorkforceSourcePermission){
 if(permission&&!hasPermission(actor,permission))return {where:'FALSE',args:[] as (string|number|null)[]};
 const iam=actor.iam,authority=actor.authority,now='floor(extract(epoch FROM clock_timestamp())*1000)::bigint';
 const args:(string|number|null)[]=[actor.assignmentId,actor.userId,actor.role,authority?.assignmentVersion??iam?.queuedAssignmentVersion??null,authority?.assignmentVersion??iam?.queuedAssignmentVersion??null,iam?.roleRevision??null,iam?.roleRevision??null,authority?.sessionId??null,actor.companyId,actor.userId,iam?.identityVersion??null,actor.companyId,actor.userId,iam?.identityVersion??null,iam?.emergencyUntil??null,iam?.emergencyUntil??null];
 let where=`EXISTS(SELECT 1 FROM assignments g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=${alias}.company_id AND (?::integer IS NULL OR g.version=?) AND iam_assignment_active(g.id,${now}) AND iam_principal_active(g.user_id,g.company_id,${now}) AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=${alias}.branch_id OR g.scope_kind='department' AND g.scope_node_id=${alias}.department_id) AND EXISTS(SELECT 1 FROM iam_roles role WHERE role.id=g.role AND role.company_id IN ('*',g.company_id) AND role.status='active' AND (?::integer IS NULL OR role.published_revision=?))${permission?` AND iam_role_allows(g.role,g.company_id,'${permission}')`:''})
 AND NOT EXISTS(${revokedSessionProbeSql('?','?','?')})
 AND (?::integer IS NULL OR COALESCE((${iamMemberVersionProbeSql('plain')}),0)=?)
 AND (?::bigint IS NULL OR ?::bigint>${now})`;
 if(authority){where+=` AND EXISTS(SELECT 1 FROM active_work_contexts c WHERE c.session_id=? AND c.company_id=? AND c.user_id=? AND c.assignment_id=? AND c.revision=? AND c.assignment_version=? AND c.expires_at>${now})`;args.push(authority.sessionId,actor.companyId,actor.userId,actor.assignmentId,authority.revision,authority.assignmentVersion);}
 if(iam?.hrAppointmentVersion!==undefined){where+=` AND EXISTS(${hrSeatExistsSql(organization.iam_store_authority_seats_s_from,people.iam_store_hr_people_p_join,people.iam_store_hr_engagements_e_join)} WHERE s.company_id=? AND s.kind='hr' AND s.scope_id=? AND s.version=? AND s.effective_at<=${now} AND iam_same_person(p.user_id,?,s.company_id))`;args.push(actor.companyId,actor.companyId,iam.hrAppointmentVersion,actor.userId);}
 return {where,args};
}
