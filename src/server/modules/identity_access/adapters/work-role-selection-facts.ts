import {stageIdentitySelectionPreference,identitySelectionPreference} from "../../work_projection/public/identity-selection-preferences";
import {assertion} from "../../workflow/public/commands";
import {clearIdentityCommandAssertions} from "../../workflow/public/identity-command-effects";
import {identitySnapshotSources as organizationSnapshotSources} from '../../organization/public/identity-snapshot-sources';
const organizationSources=organizationSnapshotSources();
import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import type {ActiveWorkSelection,SelectWorkRole} from "../../work_projection/public/work-role-store";
import {workRoleAuditEffect} from "../../workflow/public/work-role-audit";
export async function selectWorkRoleEffects(db:SqlDatabase,input:SelectWorkRole,now:number,key:string,auditId:string){const {sessionId,userId,companyId,assignmentId,assignmentVersion,revision,expiresAt}=input; await db.batch([
  db.prepare('INSERT INTO active_work_contexts(session_id,user_id,company_id,expires_at,updated_at) VALUES(?,?,?,?,?) ON CONFLICT DO NOTHING').bind(sessionId,userId,companyId,expiresAt,now),
  db.prepare('SELECT session_id FROM active_work_contexts WHERE session_id=? AND company_id=? FOR UPDATE').bind(sessionId,companyId),
  assertion(db,key,"work-context",` EXISTS(SELECT 1 FROM active_work_contexts WHERE session_id=? AND company_id=? AND user_id=? AND revision=?)`,[sessionId,companyId,userId,revision]),
  db.prepare('SELECT id FROM assignments WHERE id=? FOR SHARE').bind(assignmentId),
  assertion(db,key,"selected-assignment",` EXISTS(SELECT 1 FROM assignments WHERE id=? AND user_id=? AND company_id=? AND version=? AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND valid_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND EXISTS(SELECT 1 ${organizationSources.work_role_selection_facts_organization_nodes_c_from} WHERE c.id=assignments.company_id AND c.status='active') AND (scope_kind='holding' OR EXISTS(SELECT 1 ${organizationSources.work_role_selection_facts_organization_nodes_n_from} WHERE n.id=assignments.scope_node_id AND n.company_id=assignments.company_id AND n.status='active' AND (n.branch_id IS NULL OR EXISTS(SELECT 1 ${organizationSources.work_role_selection_facts_organization_nodes_b_from} WHERE b.id=n.branch_id AND b.status='active')))))`,[assignmentId,userId,companyId,assignmentVersion]),
  db.prepare('UPDATE active_work_contexts SET assignment_id=?,assignment_version=?,revision=revision+1,expires_at=?,updated_at=? WHERE session_id=? AND company_id=? AND user_id=?').bind(assignmentId,assignmentVersion,expiresAt,now,sessionId,companyId,userId),
  workRoleAuditEffect(db,input,now,key,auditId),
  ...(!input.restoring && input.rememberPreference !== false ? [stageIdentitySelectionPreference(db,userId,companyId,assignmentId,assignmentVersion,now)] : []),
  clearIdentityCommandAssertions(db,key),
 ]);
}
export function workRoleSelectionFacts(db:SqlDatabase){return {current:(sessionId:string,userId:string,companyId:string)=>db.prepare('SELECT * FROM active_work_contexts WHERE session_id=? AND user_id=? AND company_id=?').bind(sessionId,userId,companyId).first<ActiveWorkSelection>(),preference:(userId:string,companyId:string)=>identitySelectionPreference(db,userId,companyId),select:(input:SelectWorkRole,now:number,key:string,auditId:string)=>selectWorkRoleEffects(db,input,now,key,auditId)};}
