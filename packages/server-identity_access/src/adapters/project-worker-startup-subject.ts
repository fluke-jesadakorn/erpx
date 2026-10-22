import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {loadIamAuthority} from '@erpx/server-identity_access/adapters/iam-store';
import {runtimeEnv} from '../../../../packages/server-runtime/src/public/runtime';
/** Explicit already-authorized Company account only. A name/email never
 * chooses a Person, creates an account or grants Company membership. */
export async function projectWorkerStartupSubject(database:SqlDatabase,companyId:string,accountId:string){
 const members=(await database.prepare("SELECT a.* FROM assignments a WHERE a.company_id=? AND a.user_id=? AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) ORDER BY a.id").bind(companyId,accountId).all<Record<string,unknown>>()).results;
 if(!members.length)return null;
 const control=(await runtimeEnv()).CONTROL_DB,account=await control.prepare('SELECT id,name,email,"updatedAt" FROM "user" WHERE id=?').bind(accountId).first<{id:string;name:string;email:string;updatedAt:unknown}>();if(!account)return null;
 const linkage=(await database.prepare('SELECT * FROM iam_members WHERE company_id=? AND user_id=? ORDER BY user_id').bind(companyId,accountId).all()).results;
 return {active:true,personIds:linkage.flatMap(row=>typeof row.person_id==='string'?[row.person_id]:[]),id:account.id,name:account.name,email:account.email,image:JSON.stringify({account,members,linkage})};
}

/** Exact current Company membership options; search never infers a Person. */
export async function projectWorkerStartupAccountOptions(database:SqlDatabase,companyId:string,query:string){
 const subjects=(await database.prepare("SELECT DISTINCT a.user_id FROM assignments a WHERE a.company_id=? AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) ORDER BY a.user_id").bind(companyId).all<{user_id:string}>()).results;
 if(!subjects.length)return [];
 const control=(await runtimeEnv()).CONTROL_DB;
 const rows=(await control.prepare('SELECT id,name,email FROM "user" WHERE id IN ('+subjects.map(()=>'?').join(',')+') AND (?=\'\' OR position(lower(?) in lower(name||\' \'||email))>0) ORDER BY lower(name),id LIMIT 50').bind(...subjects.map(row=>row.user_id),query,query).all<{id:string;name:string;email:string}>()).results;
 return rows;
}
/** Durable HR placement rechecks its saved real author, not an invented Work
 * context. Browser-context expiry is not job authorization; explicit session
 * revocation and original IAM/assignment revisions are retained. */
export async function projectWorkerStartupJobAuthority(database:SqlDatabase,companyId:string,actorId:string,saved:Record<string,unknown>){
 const iam=saved.iam&&typeof saved.iam==='object'?saved.iam as Record<string,unknown>:{};
 const row=await database.prepare(`SELECT a.* FROM assignments a WHERE a.id=? AND a.user_id=? AND a.company_id=? AND a.role=? AND a.scope_kind='holding' AND a.version=? AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'people.manage') AND iam_role_allows(a.role,a.company_id,'organization.manage') AND NOT EXISTS(SELECT 1 FROM iam_revoked_sessions r WHERE r.company_id=a.company_id AND r.user_id=a.user_id AND r.session_id=?)`).bind(saved.assignmentId,actorId,companyId,saved.role,iam.queuedAssignmentVersion??saved.assignmentVersion??null,saved.sessionId??null).first<Record<string,unknown>>();
 if(!row)return null;
 const current=await loadIamAuthority(database,companyId,actorId,String(saved.role),typeof saved.sessionId==='string'?saved.sessionId:undefined);
 if(!current.permissions.includes('people.manage')||!current.permissions.includes('organization.manage'))return null;
 for(const field of ['identityVersion','roleRevision','hrAppointmentVersion','emergencyUntil'] as const)if(iam[field]!==undefined&&iam[field]!==current[field])return null;
 return JSON.stringify({row,current});
}

export const projectWorkerAppliedEvidence="SELECT id,company_id,actor_id,assignment_id,action,target_id,evidence FROM iam_audit";

/** Source-bound history label, never a current membership authorization. */
export async function projectWorkerStartupRetainedSubject(database:SqlDatabase,companyId:string,accountId:string){
 const current=await projectWorkerStartupSubject(database,companyId,accountId);if(current)return current;
 const control=(await runtimeEnv()).CONTROL_DB,account=await control.prepare('SELECT id,name,email,"updatedAt" FROM "user" WHERE id=?').bind(accountId).first<{id:string;name:string;email:string;updatedAt:unknown}>();if(!account)return null;
 const assignments=(await database.prepare('SELECT * FROM assignments WHERE company_id=? AND user_id=? ORDER BY id').bind(companyId,accountId).all()).results;
 return {active:false,personIds:[] as string[],id:account.id,name:account.name,email:account.email,image:JSON.stringify({account,assignments})};
}
export function projectWorkerAppliedEvidenceImage(database:SqlDatabase,companyId:string,caseId:string){return database.prepare("SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id)::text image FROM iam_audit a WHERE a.company_id=? AND a.target_id=? AND a.action='people.lifecycle.applied'").bind(companyId,caseId).first<string>('image');}
