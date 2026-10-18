import 'server-only';
import type {WorkContext} from '../../identity_access/public/authorization';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {benefitAttachmentPeopleReadImage} from '../../people_payroll/public/benefits-owner-facts';
import {benefitEligibilitySourceWitness} from '../../learning/public/benefit-source-facts';

const tables={benefit_plan:'benefit_plans',benefit_dependent:'benefit_dependents',benefit_election:'benefit_elections'} as const;
export function isBenefitAttachmentReadTarget(target:string):target is keyof typeof tables{return Object.hasOwn(tables,target);}

/** Full owner facts protect deferred and lazy reads without changing the
 * employee, document-manager or independent-review policy. */
export async function benefitAttachmentReadImage(database:SqlDatabase,actor:WorkContext,type:string,id:string){
 if(!isBenefitAttachmentReadTarget(type))return null;
 const related=type==='benefit_election'?`,
 'plan',(SELECT to_jsonb(plan) FROM benefit_plans plan WHERE plan.id=record.plan_id AND plan.company_id=record.company_id),
 'window',(SELECT to_jsonb(enrollment) FROM benefit_enrollment_windows enrollment WHERE enrollment.id=record.window_id AND enrollment.company_id=record.company_id),
 'dependents',(SELECT jsonb_agg(to_jsonb(dependent) ORDER BY dependent.id) FROM benefit_dependents dependent WHERE dependent.company_id=record.company_id AND dependent.id IN (SELECT jsonb_array_elements_text(CASE WHEN record.dependent_ids_json IS JSON ARRAY THEN record.dependent_ids_json::jsonb ELSE '[]'::jsonb END))),
 'payroll',(SELECT jsonb_agg(to_jsonb(input) ORDER BY input.id) FROM benefit_payroll_inputs input WHERE input.company_id=record.company_id AND input.election_id=record.id)` : '';
 const row=await database.prepare(`SELECT ${type==='benefit_plan'?'NULL::text':'record.person_id'} person_id,jsonb_build_object('record',to_jsonb(record)${related})::text image FROM ${tables[type]} record WHERE record.id=? AND record.company_id=?`).bind(id,actor.companyId).first<{person_id:string|null;image:string}>();
 if(!row)return null;
 return JSON.stringify({record:row.image,people:await benefitAttachmentPeopleReadImage(database,actor.companyId,row.person_id,actor.userId),eligibility:type==='benefit_election'?await benefitEligibilitySourceWitness(database,actor.companyId,id):null});
}
