import 'server-only';
import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {benefitAttachmentPeopleReadImage} from '@erpx/people-payroll-server/public/benefits-owner-facts';
import {benefitEligibilitySourceWitness} from '@erpx/learning-server/public/benefit-source-facts';

const tables={benefit_plan:'benefit_plans',benefit_dependent:'benefit_dependents',benefit_election:'benefit_elections'} as const;
export function isBenefitAttachmentReadTarget(target:string):target is keyof typeof tables{return Object.hasOwn(tables,target);}

/** Byte-identical owner-local election related-fact selectors. The point and
 * array shapes share one head renderer; generated SQL, column references and
 * bind order are unchanged. */
export const attachmentFactSelect=(key:string,projection:string,table:string,alias:string,predicate:string)=>`'${key}',(SELECT ${projection} FROM ${table} ${alias} WHERE ${predicate})`;
export const attachmentPointFact=(key:string,alias:string,table:string,fk:string)=>attachmentFactSelect(key,`to_jsonb(${alias})`,table,alias,`${alias}.id=record.${fk} AND ${alias}.company_id=record.company_id`);
export const attachmentArrayFact=(key:string,alias:string,table:string,predicate:string)=>attachmentFactSelect(key,`jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id)`,table,alias,`${alias}.company_id=record.company_id AND ${predicate}`);
const dependentJsonArray="dependent.id IN (SELECT jsonb_array_elements_text(CASE WHEN record.dependent_ids_json IS JSON ARRAY THEN record.dependent_ids_json::jsonb ELSE '[]'::jsonb END))";

/** Full owner facts protect deferred and lazy reads without changing the
 * employee, document-manager or independent-review policy. */
export async function benefitAttachmentReadImage(database:SqlDatabase,actor:WorkContext,type:string,id:string){
 if(!isBenefitAttachmentReadTarget(type))return null;
 const related=type==='benefit_election'?`,
 ${attachmentPointFact('plan','plan','benefit_plans','plan_id')},
 ${attachmentPointFact('window','enrollment','benefit_enrollment_windows','window_id')},
 ${attachmentArrayFact('dependents','dependent','benefit_dependents',dependentJsonArray)},
 ${attachmentArrayFact('payroll','input','benefit_payroll_inputs','input.election_id=record.id')}` : '';
 const row=await database.prepare(`SELECT ${type==='benefit_plan'?'NULL::text':'record.person_id'} person_id,jsonb_build_object('record',to_jsonb(record)${related})::text image FROM ${tables[type]} record WHERE record.id=? AND record.company_id=?`).bind(id,actor.companyId).first<{person_id:string|null;image:string}>();
 if(!row)return null;
 return JSON.stringify({record:row.image,people:await benefitAttachmentPeopleReadImage(database,actor.companyId,row.person_id,actor.userId),eligibility:type==='benefit_election'?await benefitEligibilitySourceWitness(database,actor.companyId,id):null});
}
