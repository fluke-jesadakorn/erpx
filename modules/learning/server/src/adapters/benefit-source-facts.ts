import "server-only";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {benefitConsumptionHead,jsonbAggImageHead} from "./learning-read-sql";

/** Owner-local byte-identical consumption witness shared by the private source
 * witness and the settlement source; one text source keeps both revocation
 * snapshots equal. */
const consumptionImage=`jsonb_build_object('receipt',to_jsonb(r),'certificate',to_jsonb(c),'handoff',to_jsonb(h),'assignment',to_jsonb(a),'revocations',(${jsonbAggImageHead('v')} FROM learning_certificate_revocations v WHERE v.certificate_id=c.id))`;

/** Benefits consumes reviewed evidence through its original included contract.
 * These private source facts never confer Learning management authority. */
export function benefitEligibilitySourceLocks(database:SqlDatabase){
 return [database.prepare("LOCK TABLE learning_courses,learning_assignments,learning_assignment_events,learning_eligibility_handoffs,learning_certificates,learning_certificate_revocations,learning_benefit_consumption_receipts IN SHARE ROW EXCLUSIVE MODE")];
}
export async function benefitEligibilitySourceWitness(database:SqlDatabase,companyId:string,electionId:string){
 return database.prepare(`${benefitConsumptionHead(`COALESCE(jsonb_agg(${consumptionImage} ORDER BY r.id),'[]'::jsonb)::text image`)} WHERE r.company_id=? AND r.election_id=?`).bind(companyId,electionId).first<string>("image");
}

export function benefitEligibilitySettlementLocks(database:SqlDatabase){
 return [database.prepare("LOCK TABLE learning_assignments,learning_eligibility_handoffs,learning_certificates,learning_certificate_revocations,learning_benefit_consumption_receipts IN SHARE MODE")];
}
/** Historical delivery proves the exact issued certificate; later expiry or
 * an audited revocation does not erase that immutable consumption history. */
export const benefitEligibilitySettlementSources={consumptions:`${benefitConsumptionHead(`r.*,
 c.id certificate_source_id,c.company_id certificate_company_id,c.person_id certificate_person_id,c.assignment_id,
 c.handoff_id certificate_handoff_id,c.source_assignment_version certificate_source_version,c.eligibility_code certificate_code,
 h.id handoff_source_id,h.company_id handoff_company_id,h.person_id handoff_person_id,h.assignment_id handoff_assignment_id,
 h.source_assignment_version handoff_source_version,h.eligibility_code handoff_code,
 a.id assignment_source_id,a.company_id assignment_company_id,a.version assignment_version,a.status assignment_status,
 ${consumptionImage} image`,'\n ','\n ')}`} as const;
