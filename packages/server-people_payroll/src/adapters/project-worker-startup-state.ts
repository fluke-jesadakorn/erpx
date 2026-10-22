import 'server-only';
import {createHash} from 'node:crypto';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {projectWorkerStartupSubject,projectWorkerStartupRetainedSubject} from '@erpx/server-identity_access/public/project-worker-startup-subject';
import {projectWorkerOrganizationSource} from '@erpx/server-organization/public/project-worker-startup-sources';
import {workerStartupSourcePolicy,workerStartupMarkedRows,type WorkerStartupConsumer} from '@erpx/server-people_payroll/adapters/project-worker-startup-source-sql';
import type {ProjectWorkerSelection} from '@erpx/server-people_payroll/domain/project-worker-startup';
export const parseJson=<T>(value:unknown):T=>typeof value==='string'?JSON.parse(value) as T:value as T;
export async function projectWorkerState(database:SqlDatabase,actor:WorkContext,selection:ProjectWorkerSelection,consumer:WorkerStartupConsumer='projects'){
 const policy=workerStartupSourcePolicy(consumer),projectWorkerSourceRows=policy.sourceRows,projectWorkerPlacementRows=policy.placementRows,projectWorkerMarkedRows=workerStartupMarkedRows;
 let personId='personId' in selection?selection.personId:undefined,accountId='accountId' in selection?selection.accountId:undefined;
 if(personId&&!accountId){const person=await database.prepare('SELECT user_id FROM hr_people WHERE id=? AND company_id=? AND merged_into_id IS NULL').bind(personId,actor.companyId).first<{user_id:string|null}>();const retained=await database.prepare(`SELECT account_id FROM (${projectWorkerSourceRows}) source WHERE source.person_id=? AND source.company_id=?`).bind(personId,actor.companyId).first<{account_id:string}>();accountId=retained?.account_id??person?.user_id??undefined;}
 const retainedSelection='personId'in selection&&!('accountId'in selection);const account=accountId?await (retainedSelection?projectWorkerStartupRetainedSubject:projectWorkerStartupSubject)(database,actor.companyId,accountId):null;
 const people=account?(await database.prepare('SELECT * FROM hr_people WHERE company_id=? AND merged_into_id IS NULL AND (user_id=? OR lower(email)=lower(?) OR id=?) ORDER BY id').bind(actor.companyId,account.id,account.email,personId??'').all<Record<string,unknown>>()).results:personId?(await database.prepare('SELECT * FROM hr_people WHERE company_id=? AND id=? AND merged_into_id IS NULL').bind(actor.companyId,personId).all<Record<string,unknown>>()).results:[];
 if(!personId&&account){const linked=people.filter(row=>row.user_id===account.id||account.personIds.includes(String(row.id)));if(linked.length===1)personId=String(linked[0].id);}
 const person=people.find(row=>row.id===personId)??null;
 const engagements=person?(await database.prepare("SELECT * FROM hr_engagements WHERE company_id=? AND person_id=? AND status!='ended' ORDER BY id").bind(actor.companyId,person.id).all<Record<string,unknown>>()).results:[];
 const engagement=engagements.length===1?engagements[0]:null;
 const [organization,managers,starter,placements,marked]=await Promise.all([
  projectWorkerOrganizationSource(database,actor.companyId),
  database.prepare("SELECT p.id,p.legal_name name,to_jsonb(p)::text person_source,to_jsonb(e)::text engagement_source FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=? AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' ORDER BY p.id,e.id").bind(actor.companyId).all<{id:string;name:string;person_source:string;engagement_source:string}>(),
  person?database.prepare(`SELECT * FROM (${projectWorkerSourceRows}) source WHERE source.company_id=? AND source.person_id=? ORDER BY source.command_id`).bind(actor.companyId,person.id).all<{command_id:string;request_hash:string;engagement_id:string;account_id:string;proof:unknown}>():Promise.resolve({results:[]}),
  engagement?database.prepare(`SELECT c.*,source.command_id source_command_id,source.proof source_proof FROM people_lifecycle_cases c LEFT JOIN (${projectWorkerPlacementRows}) source ON source.id=c.id AND source.company_id=c.company_id WHERE c.company_id=? AND c.engagement_id=? ORDER BY c.effective_at,c.id`).bind(actor.companyId,engagement.id).all<Record<string,unknown>>():Promise.resolve({results:[]}),
  person?database.prepare(`SELECT 1 FROM (${projectWorkerMarkedRows}) source WHERE source.person_id=?`).bind(person.id).first():null,
 ]);
 const source=starter.results.length===1?starter.results[0]:null;
 const image=person?await projectWorkerPersonImage(database,actor.companyId,String(person.id)):JSON.stringify(people);
 return {retainedSelection,account,person,people,engagement,engagements,organization,managers:managers.results,source,starterCount:starter.results.length,placements:placements.results,marked:!!marked,image};
}
export async function projectWorkerPersonImage(database:SqlDatabase,companyId:string,personId:string){return database.prepare(`SELECT jsonb_build_object(
 'person',(SELECT to_jsonb(p) FROM hr_people p WHERE p.id=? AND p.company_id=?),
 'engagements',(SELECT jsonb_agg(to_jsonb(e) ORDER BY e.id) FROM hr_engagements e WHERE e.person_id=? AND e.company_id=?),
 'employees',(SELECT jsonb_agg(to_jsonb(e) ORDER BY e.id) FROM employees e WHERE e.person_id=? AND e.company_id=?),
 'cases',(SELECT jsonb_agg(to_jsonb(c) ORDER BY c.id) FROM people_lifecycle_cases c WHERE c.person_id=? AND c.company_id=?),
 'tasks',(SELECT jsonb_agg(to_jsonb(t) ORDER BY t.id) FROM people_preparation_tasks t JOIN people_lifecycle_cases c ON c.id=t.case_id AND c.company_id=t.company_id WHERE c.person_id=? AND c.company_id=?),
 'compensation',(SELECT jsonb_agg(to_jsonb(c) ORDER BY c.id) FROM hr_compensation_revisions c JOIN hr_engagements e ON e.id=c.engagement_id WHERE e.person_id=? AND e.company_id=?),
 'leave',(SELECT jsonb_agg(to_jsonb(l) ORDER BY l.id) FROM leave_policy_assignments l JOIN hr_engagements e ON e.id=l.engagement_id WHERE e.person_id=? AND e.company_id=?)
 )::text image`).bind(...Array.from({length:7},()=>[personId,companyId]).flat()).first<string>('image');}
export type ProjectWorkerState=Awaited<ReturnType<typeof projectWorkerState>>;
export function projectWorkerContext(actor:WorkContext,state:ProjectWorkerState,consumer:WorkerStartupConsumer='projects'){return createHash('sha256').update(JSON.stringify([...(consumer==='learn'?['learn']:[]),actor.holdingId,actor.companyId,actor.userId,actor.assignmentId,actor.role,actor.authority??null,actor.iam,state.account?.image,state.people,state.engagements,state.organization.image,state.managers,state.source,state.placements])).digest('hex');}
