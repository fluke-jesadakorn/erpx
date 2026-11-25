import {ppRd36TwoPredTail} from './pp-read-tail-r36-sources';
import 'server-only';
import {createHash} from 'node:crypto';
import {nativePeoplePayrollReadAll,PEOPLE_PAYROLL_RD_PROJECT_WORKER_PERSON_USER} from './native-people-payroll-read';
import {personImage,type Lane9Actor} from './native-lane-9/client';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {projectWorkerStartupSubject,projectWorkerStartupRetainedSubject} from '@erpx/server-identity_access/public/project-worker-startup-subject';
import {projectWorkerOrganizationSource} from '@erpx/server-organization/public/project-worker-startup-sources';
import {workerStartupSourcePolicy,workerStartupMarkedRows,type WorkerStartupConsumer} from '@erpx/server-people_payroll/adapters/project-worker-startup-source-sql';
import type {ProjectWorkerSelection} from '@erpx/server-people_payroll/domain/project-worker-startup';
export const parseJson=<T>(value:unknown):T=>typeof value==='string'?JSON.parse(value) as T:value as T;
export async function projectWorkerPersonUser(database:SqlDatabase,personId:string,companyId:string){return (await nativePeoplePayrollReadAll<{user_id:string|null}>(PEOPLE_PAYROLL_RD_PROJECT_WORKER_PERSON_USER,[personId,companyId],database)).results[0]??null;}
export async function projectWorkerState(database:SqlDatabase,actor:WorkContext,selection:ProjectWorkerSelection,consumer:WorkerStartupConsumer='projects'){
 const policy=workerStartupSourcePolicy(consumer),projectWorkerSourceRows=policy.sourceRows,projectWorkerPlacementRows=policy.placementRows,projectWorkerMarkedRows=workerStartupMarkedRows;
 let personId='personId' in selection?selection.personId:undefined,accountId='accountId' in selection?selection.accountId:undefined;
 if(personId&&!accountId){const person=await projectWorkerPersonUser(database,personId,actor.companyId);const retained=await database.prepare(`SELECT account_id FROM (${projectWorkerSourceRows}) source WHERE source.person_id=? AND source.company_id=?`).bind(personId,actor.companyId).first<{account_id:string}>();accountId=retained?.account_id??person?.user_id??undefined;}
 const retainedSelection='personId'in selection&&!('accountId'in selection);const account=accountId?await (retainedSelection?projectWorkerStartupRetainedSubject:projectWorkerStartupSubject)(database,actor.companyId,accountId):null;
 const people=account?(await database.prepare('SELECT * FROM hr_people WHERE company_id=? AND merged_into_id IS NULL AND (user_id=? OR lower(email)=lower(?) OR id=?) ORDER BY id').bind(actor.companyId,account.id,account.email,personId??'').all<Record<string,unknown>>()).results:personId?(await database.prepare(ppRd36TwoPredTail("*", "hr_people", "company_id", "id", " AND merged_into_id IS NULL")).bind(actor.companyId,personId).all<Record<string,unknown>>()).results:[];
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
 const image=person?await projectWorkerPersonImage(database,actor.companyId,String(person.id),actor):JSON.stringify(people);
 return {retainedSelection,account,person,people,engagement,engagements,organization,managers:managers.results,source,starterCount:starter.results.length,placements:placements.results,marked:!!marked,image};
}
/** Native lane 9: the Company-scoped statement, projection and response shape
 * are owned by the Rust axum/sqlx service; this adapter only transports the
 * bounded image and fails closed when the native runtime is unavailable. */
export async function projectWorkerPersonImage(database:SqlDatabase,companyId:string,personId:string,actor?:Lane9Actor){return personImage(database,companyId,personId,actor);}
export type ProjectWorkerState=Awaited<ReturnType<typeof projectWorkerState>>;
export function projectWorkerContext(actor:WorkContext,state:ProjectWorkerState,consumer:WorkerStartupConsumer='projects'){return createHash('sha256').update(JSON.stringify([...(consumer==='learn'?['learn']:[]),actor.holdingId,actor.companyId,actor.userId,actor.assignmentId,actor.role,actor.authority??null,actor.iam,state.account?.image,state.people,state.engagements,state.organization.image,state.managers,state.source,state.placements])).digest('hex');}
