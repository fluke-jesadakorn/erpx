import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function activeBenefitPerson(db:SqlDatabase,companyId:string,userId:string){return db.prepare("SELECT p.id,e.id engagement_id,e.branch_id,e.department_id FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=? AND p.user_id=? AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' LIMIT 1").bind(companyId,userId).first<{id:string;engagement_id:string;branch_id:string|null;department_id:string|null}>();}
export const benefitElectionEmployeeName=()=>"SELECT legal_name FROM hr_people WHERE id=e.person_id";
export const benefitDependentParticipants=()=>"JOIN hr_people p ON p.id=d.person_id AND p.company_id=d.company_id LEFT JOIN LATERAL (SELECT x.branch_id,x.department_id FROM hr_engagements x WHERE x.person_id=p.id AND x.company_id=p.company_id ORDER BY CASE WHEN x.status='active' THEN 0 ELSE 1 END,x.start_date DESC,x.id LIMIT 1) e ON true";
export const benefitElectionParticipants=()=>"JOIN hr_people p ON p.id=x.person_id AND p.company_id=x.company_id JOIN hr_engagements e ON e.id=x.engagement_id AND e.company_id=x.company_id";

/** Benefits leases these included worker references before the first command
 * snapshot. It does not require a separate People-directory purchase. */
export function benefitPeopleSourceLocks(database:SqlDatabase){return [database.prepare("LOCK TABLE hr_people,hr_engagements IN SHARE MODE")];}
export async function benefitPeopleSourceWitness(database:SqlDatabase,companyId:string,personId:string){
 return database.prepare(`SELECT jsonb_build_object('person',jsonb_build_object('id',p.id,'companyId',p.company_id,'userId',p.user_id,'status',p.status,'version',p.version,'mergedIntoId',p.merged_into_id),'engagements',(SELECT jsonb_agg(jsonb_build_object('id',e.id,'companyId',e.company_id,'personId',e.person_id,'branchId',e.branch_id,'departmentId',e.department_id,'status',e.status,'version',e.version,'startDate',e.start_date,'endDate',e.end_date) ORDER BY e.id) FROM hr_engagements e WHERE e.person_id=p.id AND e.company_id=p.company_id))::text image FROM hr_people p WHERE p.company_id=? AND p.id=?`).bind(companyId,personId).first<string>("image");
}

/** Closed minimal relations for the original Benefits sensitive-file policy. */
export const benefitAttachmentPeopleSources={
 people:"SELECT id,company_id,user_id,status,version,merged_into_id FROM hr_people",
 engagements:"SELECT id,person_id,company_id,branch_id,department_id,status,version,start_date FROM hr_engagements",
} as const;

/** Complete point facts stay inside the Benefits private read transaction.
 * Included worker references do not confer People-directory access. */
export function benefitAttachmentPeopleReadImage(database:SqlDatabase,companyId:string,personId:string|null,userId:string){
 return database.prepare(`SELECT COALESCE(jsonb_agg(jsonb_build_object('person',to_jsonb(person),
 'engagements',(SELECT jsonb_agg(to_jsonb(engagement) ORDER BY engagement.id) FROM hr_engagements engagement WHERE engagement.person_id=person.id AND engagement.company_id=person.company_id)) ORDER BY person.id),'[]'::jsonb)::text image
 FROM hr_people person WHERE person.company_id=? AND (person.id=? OR person.user_id=?)`).bind(companyId,personId,userId).first<string>('image');
}
