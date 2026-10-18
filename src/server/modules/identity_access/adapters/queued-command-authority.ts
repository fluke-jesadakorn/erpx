import 'server-only';
import type {WorkContext} from '../public/authorization';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import {hasPermission} from '@/lib/identity_access/permissions';
import {loadIamAuthority} from './iam-store';
import {assignmentReadSql,currentAssignmentReadScope} from './record-source-authority';
import {workflowActiveContext,workflowActiveGrant} from './workflow-command-authority';
import {lockQueuedOrganizationAuthority} from '../../organization/public/queued-command-authority';
import {lockQueuedPeopleAuthority} from '../../people_payroll/public/queued-command-authority';
/** Cover inserted and changed authority sources before any owner snapshot. */
export async function lockQueuedCommandAuthority(database:SqlDatabase){
 await database.prepare('LOCK TABLE assignments,iam_roles,iam_role_revisions,iam_members,iam_identities,iam_connections,iam_revoked_sessions,iam_emergency_sessions,active_work_contexts,iam_access_subjects,um_attributes,um_values,um_groups,um_memberships,um_policies,um_attachments,um_candidates IN SHARE MODE').run();
 await lockQueuedOrganizationAuthority(database);await lockQueuedPeopleAuthority(database);
}
/** Read-only after the owner's deferred-effects flush. The caller retains the
 * source leases; no role is selected or fabricated for this queued identity. */
async function queuedAuthority(database:SqlDatabase,actor:WorkContext,permission?:string){
 const scope=permission?{where:assignmentReadSql('queued_scope',permission),args:[actor.assignmentId,actor.userId,actor.role]}:currentAssignmentReadScope(actor,'workforce_actor_scope'),alias=permission?'queued_scope':'workforce_actor_scope';
 const row=await database.prepare(`SELECT ${scope.where} AND NOT EXISTS(SELECT 1 FROM iam_revoked_sessions r WHERE r.company_id=${alias}.company_id AND r.user_id=? AND r.session_id=?) AND (?::bigint IS NULL OR EXISTS(SELECT 1 FROM assignments a WHERE a.id=? AND a.version=?)) allowed FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) ${alias}`).bind(...scope.args,actor.userId,actor.authority?.sessionId??null,actor.iam?.queuedAssignmentVersion??null,actor.assignmentId,actor.iam?.queuedAssignmentVersion??null,actor.companyId,actor.branchId,actor.departmentId).first<boolean>('allowed');
 if(!row)throw new ApiError(403,'QUEUED_AUTHORITY_CHANGED','The original queued assignment or principal is no longer current.');
 if(actor.authority){const context=await workflowActiveContext(database,actor);if(!context||context.revision!==actor.authority.revision||context.assignment_id!==actor.assignmentId||context.assignment_version!==actor.authority.assignmentVersion||!await workflowActiveGrant(database,actor,Date.now()))throw new ApiError(403,'QUEUED_AUTHORITY_CHANGED','The original queued Work context changed.');}
 const current=await loadIamAuthority(database,actor.companyId,actor.userId,actor.role,actor.authority?.sessionId);
 for(const field of ['identityVersion','roleRevision','hrAppointmentVersion','emergencyUntil'] as const)if(actor.iam?.[field]!==undefined&&actor.iam[field]!==current[field])throw new ApiError(403,'QUEUED_AUTHORITY_CHANGED','The original queued IAM authority changed.');
 if(permission&&!hasPermission({...actor,iam:current},permission))throw new ApiError(403,'QUEUED_AUTHORITY_CHANGED','The original queued permission is unavailable.');
 const version=await database.prepare('SELECT version FROM assignments WHERE id=? AND company_id=? AND user_id=? AND role=?').bind(actor.assignmentId,actor.companyId,actor.userId,actor.role).first<number>('version');
 if(!version)throw new ApiError(403,'QUEUED_AUTHORITY_CHANGED','The original queued assignment is unavailable.');
 return {...actor,iam:{...current,queuedAssignmentVersion:version}};
}
export function requireQueuedCommandAuthority(database:SqlDatabase,actor:WorkContext,permission:string){return queuedAuthority(database,actor,permission);}
/** Permission-free account binding for an original owner operation that has
 * no named permission (for example its own Calendar invitation). The owner
 * still repeats its actual creator/organizer/attendee record policy. */
export function requireQueuedAccountAuthority(database:SqlDatabase,actor:WorkContext){return queuedAuthority(database,actor);}
