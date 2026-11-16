import {identitySnapshotSources as organizationSnapshotSources} from '@erpx/server-organization/public/identity-snapshot-sources';
const organizationSources=organizationSnapshotSources();
import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import type {CommandActor} from "@erpx/server-workflow/public/commands-contract";
export type WorkflowAssertionPort=(db:SqlDatabase,id:string,step:string,query:string,args?:(string|number|null)[])=>SqlStatement;
export function createWorkflowCommandGuards(assertion:WorkflowAssertionPort){
 if(typeof assertion!=='function')throw new Error('WORKFLOW_ASSERTION_PORT_REQUIRED');
function workflowWorkContextGuards(db:SqlDatabase,actor:CommandActor,id:string):SqlStatement[]{
 return (actor.authority ? [
       db.prepare('SELECT session_id FROM active_work_contexts WHERE session_id=? AND company_id=? FOR UPDATE').bind(actor.authority.sessionId,actor.companyId),
       assertion(db,id,'work-context',`EXISTS(SELECT 1 FROM active_work_contexts WHERE session_id=? AND company_id=? AND user_id=? AND assignment_id=? AND revision=? AND assignment_version=? AND expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint)`,[actor.authority.sessionId,actor.companyId,actor.userId,actor.assignmentId,actor.authority.revision,actor.authority.assignmentVersion]),
       db.prepare('SELECT id FROM assignments WHERE id=? FOR SHARE').bind(actor.assignmentId),
       assertion(db,id,'assignment-version',`EXISTS(SELECT 1 FROM assignments a WHERE a.id=? AND a.version=? AND a.user_id=? AND a.role=? AND a.company_id=?
         AND a.revoked_at IS NULL AND a.valid_from<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND a.valid_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint
         AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=?) OR (a.scope_kind='department' AND a.scope_node_id=?))
         AND EXISTS(SELECT 1 ${organizationSources.workflow_command_authority_organization_nodes_c_from} WHERE c.id=a.company_id AND c.status='active')
         AND (a.scope_kind='holding' OR EXISTS(SELECT 1 ${organizationSources.workflow_command_authority_organization_nodes_n_from} WHERE n.id=a.scope_node_id AND n.company_id=a.company_id AND n.status='active'
           AND (n.branch_id IS NULL OR EXISTS(SELECT 1 ${organizationSources.workflow_command_authority_organization_nodes_b_from} WHERE b.id=n.branch_id AND b.company_id=a.company_id AND b.status='active')))))`,
         [actor.assignmentId,actor.authority.assignmentVersion,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId]),
      ]:[]);
}
function workflowAssignmentGuard(db:SqlDatabase,actor:CommandActor,id:string){return assertion(db,id,"assignment", "EXISTS(SELECT 1 FROM assignments WHERE id=? AND user_id=? AND role=? AND company_id=? AND (scope_kind='holding' OR (scope_kind='branch' AND scope_node_id=?) OR (scope_kind='department' AND scope_node_id=?)) AND revoked_at IS NULL AND valid_from<=(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint))", [actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId]);}
 return Object.freeze({workflowWorkContextGuards,workflowAssignmentGuard});
}
export function workflowAssignmentAuthorityProjection(){return "(SELECT jsonb_build_object('scopeKind',scope_kind,'scopeNodeId',scope_node_id) FROM assignments WHERE id=?)";}
export function workflowActiveContext(db:SqlDatabase,actor:CommandActor){return db.prepare('SELECT revision,assignment_id,assignment_version FROM active_work_contexts WHERE session_id=? AND company_id=?').bind(actor.authority!.sessionId,actor.companyId).first<{revision:number;assignment_id:string;assignment_version:number}>();}
export function workflowActiveGrant(db:SqlDatabase,actor:CommandActor,now:number){return db.prepare("SELECT 1 FROM assignments a JOIN active_work_contexts c ON c.assignment_id=a.id WHERE c.session_id=? AND c.company_id=? AND c.user_id=? AND a.id=? AND a.user_id=? AND a.role=? AND a.version=? AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>? AND c.expires_at>?").bind(actor.authority!.sessionId,actor.companyId,actor.userId,actor.assignmentId,actor.userId,actor.role,actor.authority!.assignmentVersion,now,now,now).first();}
