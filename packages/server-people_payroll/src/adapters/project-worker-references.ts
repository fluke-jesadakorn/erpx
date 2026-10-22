import 'server-only';
import {createHash} from 'node:crypto';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';

/** Projects includes worker references; it never acquires private HR authority.
 * The original Operations activeStaff owner still decides actual eligibility,
 * including account linking and cross-branch project membership. */
export function projectWorkerReferenceSourceReadLocks(database:SqlDatabase){
 return [database.prepare('LOCK TABLE hr_people,hr_engagements IN SHARE MODE')];
}
export async function projectWorkerReferenceWitness(database:SqlDatabase,companyId:string,userIds:readonly string[]){
 const users=JSON.stringify([...new Set(userIds)].sort()),image=await database.prepare(`SELECT jsonb_agg(jsonb_build_object('person',to_jsonb(p),
 'engagements',(SELECT jsonb_agg(to_jsonb(e) ORDER BY e.id) FROM hr_engagements e WHERE e.person_id=p.id AND e.company_id=p.company_id)) ORDER BY p.id)::text image
 FROM hr_people p WHERE p.company_id=? AND EXISTS(SELECT 1 FROM jsonb_array_elements_text(?::jsonb) worker(user_id) WHERE iam_same_person(p.user_id,worker.user_id,p.company_id))`).bind(companyId,users).first<string|null>('image');
 return createHash('sha256').update(image??'null').digest('hex');
}
