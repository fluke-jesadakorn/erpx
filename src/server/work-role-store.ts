import type { SqlDatabase } from './runtime/types';
import { ApiError } from './http';
export type ActiveWorkSelection = { session_id:string;user_id:string;company_id:string;assignment_id:string|null;assignment_version:number|null;revision:number;expires_at:number };
export type WorkAuthority = { sessionId:string;revision:number;assignmentVersion:number };
export async function readWorkSelection(db:SqlDatabase,sessionId:string,userId:string,companyId:string) {
 return db.prepare('SELECT * FROM active_work_contexts WHERE session_id=? AND user_id=? AND company_id=?').bind(sessionId,userId,companyId).first<ActiveWorkSelection>();
}
export async function selectWorkRole(db:SqlDatabase, input:{sessionId:string;userId:string;companyId:string;assignmentId:string;assignmentVersion:number;revision:number;expiresAt:number;restoring?:boolean}) {
 const {sessionId,userId,companyId,assignmentId,assignmentVersion,revision,expiresAt}=input,now=Date.now();
 const key=crypto.randomUUID();
 try {
 await db.batch([
  db.prepare('INSERT INTO active_work_contexts(session_id,user_id,company_id,expires_at,updated_at) VALUES(?,?,?,?,?) ON CONFLICT DO NOTHING').bind(sessionId,userId,companyId,expiresAt,now),
  db.prepare('SELECT session_id FROM active_work_contexts WHERE session_id=? AND company_id=? FOR UPDATE').bind(sessionId,companyId),
  db.prepare(`INSERT INTO command_assertions(command_id,step,ok) SELECT ?,'work-context',CASE WHEN EXISTS(SELECT 1 FROM active_work_contexts WHERE session_id=? AND company_id=? AND user_id=? AND revision=?) THEN 1 ELSE 0 END`).bind(key,sessionId,companyId,userId,revision),
  db.prepare('SELECT id FROM assignments WHERE id=? FOR SHARE').bind(assignmentId),
  db.prepare(`INSERT INTO command_assertions(command_id,step,ok) SELECT ?,'selected-assignment',CASE WHEN EXISTS(SELECT 1 FROM assignments WHERE id=? AND user_id=? AND company_id=? AND version=? AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND valid_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND EXISTS(SELECT 1 FROM organization_nodes c WHERE c.id=assignments.company_id AND c.status='active') AND (scope_kind='holding' OR EXISTS(SELECT 1 FROM organization_nodes n WHERE n.id=assignments.scope_node_id AND n.company_id=assignments.company_id AND n.status='active' AND (n.branch_id IS NULL OR EXISTS(SELECT 1 FROM organization_nodes b WHERE b.id=n.branch_id AND b.status='active'))))) THEN 1 ELSE 0 END`).bind(key,assignmentId,userId,companyId,assignmentVersion),
  db.prepare('UPDATE active_work_contexts SET assignment_id=?,assignment_version=?,revision=revision+1,expires_at=?,updated_at=? WHERE session_id=? AND company_id=? AND user_id=?').bind(assignmentId,assignmentVersion,expiresAt,now,sessionId,companyId,userId),
  db.prepare(`INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) SELECT ?,?,?,?,'work.role.select','work_context',?,?,?, ?,jsonb_build_object('role',role,'assignmentId',id,'assignmentVersion',version,'companyId',company_id,'scopeKind',scope_kind,'scopeNodeId',scope_node_id,'revision',?::bigint)::text FROM assignments WHERE id=?`).bind(crypto.randomUUID(),key,userId,assignmentId,companyId,revision+1,input.restoring ? "Restored saved work assignment for a new session" : "User selected their active work assignment",now,revision+1,assignmentId),
  ...(!input.restoring ? [db.prepare(`INSERT INTO work_role_preferences(user_id,company_id,assignment_id,assignment_version,updated_at) VALUES(?,?,?,?,?) ON CONFLICT(user_id,company_id) DO UPDATE SET assignment_id=excluded.assignment_id,assignment_version=excluded.assignment_version,updated_at=excluded.updated_at`).bind(userId,companyId,assignmentId,assignmentVersion,now)] : []),
  db.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key),
 ]);
 } catch(error) { if ((error as {code?:string}).code==='23514') throw new ApiError(409,'WORK_CONTEXT_CHANGED','Your access or work role changed. Review and choose again.');throw error; }
 return (await readWorkSelection(db,sessionId,userId,companyId))!;
}

// Saved preferences initialize a new session; they never replace a live session's
// role or revision, so another device cannot change an open form's authority.
export async function restoreWorkSelection(db:SqlDatabase,input:{sessionId:string;userId:string;companyId:string;expiresAt:number},assignments:{id:string;version?:number}[]) {
 const current=await readWorkSelection(db,input.sessionId,input.userId,input.companyId);
 if(current)return current;
 const preference=await db.prepare('SELECT assignment_id,assignment_version FROM work_role_preferences WHERE user_id=? AND company_id=?').bind(input.userId,input.companyId).first<{assignment_id:string;assignment_version:number}>();
 const assignment=preference && assignments.find(a=>a.id===preference.assignment_id && a.version===preference.assignment_version);
 if(!assignment)return null;
 try{return await selectWorkRole(db,{...input,assignmentId:assignment.id,assignmentVersion:assignment.version!,revision:0,restoring:true});}
 catch(error){
  if(error instanceof ApiError && error.code==='WORK_CONTEXT_CHANGED')return readWorkSelection(db,input.sessionId,input.userId,input.companyId);
  throw error;
 }
}
