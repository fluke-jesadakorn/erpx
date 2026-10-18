import 'server-only';
import {createHash} from 'node:crypto';
import type {WorkContext} from '../../identity_access/public/authorization';
import type {SqlDatabase} from '../../runtime/public/runtime/types';

/** Learning's included learner foundation exposes identity and placement only.
 * Its original reviewer scope applies before rows are returned or selected.
 * Learning owns the current IAM and purchased-capability decision; no People
 * directory, compensation, contact or lifecycle authority is implied. */
const placement=`p.company_id=? AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active'
 AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)`;
const args=(actor:WorkContext)=>[actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId];
export function learningWorkerReferenceSourceReadLocks(database:SqlDatabase){
 return [database.prepare('LOCK TABLE hr_people,hr_engagements IN SHARE MODE')];
}
export async function learningWorkerChoices(database:SqlDatabase,actor:WorkContext){
 return (await database.prepare(`SELECT p.user_id id,p.legal_name name,e.branch_id,e.department_id
 FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id
 WHERE ${placement} AND p.user_id IS NOT NULL ORDER BY p.legal_name,p.id,e.id`).bind(...args(actor))
  .all<{id:string;name:string;branch_id:string|null;department_id:string|null}>()).results;
}
export function learningWorkerPlacement(database:SqlDatabase,actor:WorkContext,userId:string){
 return database.prepare(`SELECT p.id,p.company_id,e.branch_id,e.department_id
 FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id
 WHERE ${placement} AND p.user_id=? ORDER BY p.id,e.id LIMIT 1`).bind(...args(actor),userId)
  .first<{id:string;company_id:string;branch_id:string|null;department_id:string|null}>();
}
/** Opaque, server-only full source image. A legitimate Learning mutation never
 * changes its worker's native People records. Include inactive/merged records
 * so a concurrent or deferred eligibility change cannot disappear as zero. */
export async function learningWorkerReferenceWitness(database:SqlDatabase,companyId:string,userId:string|null=null){
 const image=await database.prepare(`SELECT jsonb_agg(jsonb_build_object('person',to_jsonb(p),
 'engagements',(SELECT jsonb_agg(to_jsonb(e) ORDER BY e.id) FROM hr_engagements e WHERE e.person_id=p.id AND e.company_id=p.company_id)) ORDER BY p.id)::text image
 FROM hr_people p WHERE p.company_id=? AND (?::text IS NULL OR p.user_id=?)`).bind(companyId,userId,userId).first<string|null>('image');
 return createHash('sha256').update(image??'null').digest('hex');
}
