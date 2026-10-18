import "server-only";
import {createHash} from "node:crypto";
import type {PostgresDatabase} from "../../runtime/public/runtime/postgres";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {packageSettlementSource,type PackageSettlementSourceLock} from "../../commercial/public/package-settlement-sources";
import {benefitEligibilitySettlementLocks,benefitEligibilitySettlementSources as learning} from "../../learning/public/benefit-source-facts";

type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};
const sources=new WeakMap<SqlDatabase,string>();
export async function lockBenefitPackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){
 const {database,companyId}=packageSettlementSource(owner,lock);
 await database.batch([database.prepare("LOCK TABLE benefit_plans,benefit_plan_events,benefit_enrollment_windows,benefit_window_events,benefit_dependents,benefit_dependent_events,benefit_elections,benefit_election_events,benefit_payroll_inputs,command_receipts IN SHARE MODE"),...benefitEligibilitySettlementLocks(database)]);
 sources.set(database,companyId);
}
export function assertBenefitPackageObligationSources(owner:PostgresDatabase,database:SqlDatabase,companyId:string){owner.assertActiveTransaction(database);if(sources.get(database)!==companyId)throw Error("BENEFIT_PACKAGE_OBLIGATION_SOURCE_REQUIRED");}
function terminal(table:string,id:string,kind:string){
 return `EXISTS(SELECT 1 FROM ${table} event JOIN command_receipts command ON command.record_id=${id}.id
 AND command.operation='benefits.${kind}.'||CASE WHEN event.action IN ('create','elect') THEN 'create' ELSE event.action END
 AND command.actor_id=event.actor_id AND command.authority_json::jsonb->>'companyId'=${id}.company_id
 AND command.result_json::jsonb->>'id'=${id}.id AND command.result_json::jsonb->>'status'=event.status
 AND command.result_json::jsonb->>'version'=event.version::text
 WHERE event.${kind==='window'?'window':kind}_id=${id}.id AND event.status=${id}.status AND event.version<=${id}.version)`;
}
/** Closed actual owner lineage, including sources hidden by a user's Work,
 * purchase, state or rollout. Peer owners never substitute caller labels. */
export const benefitPackageSettlementSources={
 plans:`SELECT p.*,${terminal('benefit_plan_events','p','plan')} terminal_proven FROM benefit_plans p`,
 windows:`SELECT w.*,${terminal('benefit_window_events','w','window')} terminal_proven FROM benefit_enrollment_windows w`,
 dependents:`SELECT d.*,${terminal('benefit_dependent_events','d','dependent')} terminal_proven FROM benefit_dependents d`,
 elections:`SELECT e.*,${terminal('benefit_election_events','e','election')} terminal_proven FROM benefit_elections e`,
 inputs:"SELECT * FROM benefit_payroll_inputs",
} as const;
export async function benefitPackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 assertBenefitPackageObligationSources(owner,database,input.companyId);
 if(input.moduleId!=="benefits"||input.packageId!=="people-benefits"||input.packageVersion!==1||!Number.isSafeInteger(input.now)||input.now<0)throw Error("BENEFIT_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
 const facts=(await database.prepare(`WITH scope AS(SELECT ?::text company_id,to_char(to_timestamp(?::bigint/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD') today),
 plans AS MATERIALIZED(SELECT p.* FROM (${benefitPackageSettlementSources.plans}) p,scope WHERE p.company_id=scope.company_id),
 windows AS MATERIALIZED(SELECT w.* FROM (${benefitPackageSettlementSources.windows}) w,scope WHERE w.company_id=scope.company_id),
 elections AS MATERIALIZED(SELECT e.* FROM (${benefitPackageSettlementSources.elections}) e,scope WHERE e.company_id=scope.company_id),
 facts AS(
 SELECT 'plan' kind,p.id,p.version,p.status state,to_jsonb(p) image,p.status NOT IN ('retired','rejected') OR NOT p.terminal_proven pending FROM plans p
 UNION ALL SELECT 'window',w.id,w.version,w.status,to_jsonb(w),w.status NOT IN ('closed','cancelled') OR NOT w.terminal_proven FROM windows w
 UNION ALL SELECT 'dependent',d.id,d.version,d.status,to_jsonb(d),d.status='pending' OR NOT d.terminal_proven FROM (${benefitPackageSettlementSources.dependents}) d,scope WHERE d.company_id=scope.company_id
 UNION ALL SELECT 'election',e.id,e.version,e.status,to_jsonb(e),e.status NOT IN ('released','cancelled') OR NOT e.terminal_proven
 OR plan.id IS NULL OR plan.company_id IS DISTINCT FROM e.company_id OR plan.version<e.plan_version
 OR win.id IS NULL OR win.company_id IS DISTINCT FROM e.company_id OR win.version<e.window_version
 OR (e.status='released' AND (e.released_by IS NULL OR NOT EXISTS(SELECT 1 FROM benefit_payroll_inputs handoff JOIN benefit_election_events event ON event.election_id=e.id AND event.version=handoff.source_election_version AND event.action='release' AND event.status='released' AND event.actor_id=handoff.released_by
 JOIN command_receipts command ON command.record_id=e.id AND command.operation='benefits.election.release' AND command.actor_id=event.actor_id AND command.authority_json::jsonb->>'companyId'=e.company_id AND command.result_json::jsonb->>'version'=event.version::text
 WHERE handoff.election_id=e.id AND handoff.company_id=e.company_id AND handoff.person_id=e.person_id AND handoff.engagement_id=e.engagement_id AND handoff.currency=e.currency AND handoff.source_election_version<=e.version AND handoff.employee_deduction_minor=e.employee_deduction_minor AND handoff.employer_cost_minor=e.employer_cost_minor AND handoff.effective_from=e.effective_from AND handoff.effective_to IS NOT DISTINCT FROM e.effective_to)
 OR (e.employee_deduction_minor+e.employer_cost_minor>0 AND (e.effective_to IS NULL OR e.effective_to>=scope.today))))
 FROM elections e CROSS JOIN scope LEFT JOIN plans plan ON plan.id=e.plan_id LEFT JOIN windows win ON win.id=e.window_id
 UNION ALL SELECT 'payroll-input',h.id,h.source_election_version,h.status,to_jsonb(h),e.id IS NULL OR e.status<>'released' OR e.company_id IS DISTINCT FROM h.company_id OR e.person_id IS DISTINCT FROM h.person_id OR e.engagement_id IS DISTINCT FROM h.engagement_id OR e.version<h.source_election_version OR e.currency IS DISTINCT FROM h.currency OR e.employee_deduction_minor IS DISTINCT FROM h.employee_deduction_minor OR e.employer_cost_minor IS DISTINCT FROM h.employer_cost_minor OR e.effective_from IS DISTINCT FROM h.effective_from OR e.effective_to IS DISTINCT FROM h.effective_to FROM benefit_payroll_inputs h CROSS JOIN scope LEFT JOIN elections e ON e.id=h.election_id WHERE h.company_id=scope.company_id
 UNION ALL SELECT 'learning-consumption',c.id,c.source_assignment_version,'consumed',c.image,
 e.id IS NULL OR c.person_id IS DISTINCT FROM e.person_id OR c.certificate_id IS DISTINCT FROM e.learning_certificate_id OR c.certificate_source_id IS NULL OR c.certificate_company_id IS DISTINCT FROM c.company_id OR c.certificate_person_id IS DISTINCT FROM c.person_id OR c.certificate_handoff_id IS DISTINCT FROM c.handoff_id OR c.certificate_source_version IS DISTINCT FROM c.source_assignment_version OR c.certificate_code IS DISTINCT FROM c.eligibility_code
 OR c.handoff_source_id IS NULL OR c.handoff_company_id IS DISTINCT FROM c.company_id OR c.handoff_person_id IS DISTINCT FROM c.person_id OR c.handoff_assignment_id IS DISTINCT FROM c.assignment_id OR c.handoff_source_version IS DISTINCT FROM c.source_assignment_version OR c.handoff_code IS DISTINCT FROM c.eligibility_code OR c.assignment_source_id IS NULL OR c.assignment_company_id IS DISTINCT FROM c.company_id OR c.assignment_version<c.source_assignment_version
 FROM (${learning.consumptions}) c CROSS JOIN scope LEFT JOIN elections e ON e.id=c.election_id WHERE c.company_id=scope.company_id
 UNION ALL SELECT 'missing-learning-consumption',e.id,e.version,e.status,to_jsonb(e),true FROM elections e JOIN plans plan ON plan.id=e.plan_id WHERE e.status<>'cancelled' AND plan.required_eligibility_code IS NOT NULL AND NOT EXISTS(SELECT 1 FROM (${learning.consumptions}) c WHERE c.election_id=e.id AND c.company_id=e.company_id AND c.certificate_id=e.learning_certificate_id AND c.person_id=e.person_id AND c.eligibility_code=plan.required_eligibility_code)
 ) SELECT kind,id,version,state,image,COALESCE(pending,true) pending FROM facts ORDER BY kind,id`).bind(input.companyId,input.now).all<{kind:string;id:string;version:number;state:string;image:unknown;pending:boolean}>()).results;
 // Event history is immutable, but still participates in the reviewed image.
 const events=await database.prepare(`SELECT 'plan' kind,to_jsonb(event)::text image FROM benefit_plan_events event JOIN benefit_plans r ON r.id=event.plan_id WHERE r.company_id=? UNION ALL SELECT 'window',to_jsonb(event)::text FROM benefit_window_events event JOIN benefit_enrollment_windows r ON r.id=event.window_id WHERE r.company_id=? UNION ALL SELECT 'dependent',to_jsonb(event)::text FROM benefit_dependent_events event JOIN benefit_dependents r ON r.id=event.dependent_id WHERE r.company_id=? UNION ALL SELECT 'election',to_jsonb(event)::text FROM benefit_election_events event JOIN benefit_elections r ON r.id=event.election_id WHERE r.company_id=? ORDER BY kind,image`).bind(input.companyId,input.companyId,input.companyId,input.companyId).all();
 return {companyId:input.companyId,packageId:input.packageId,moduleId:"benefits",observedAt:input.now,revision:createHash("sha256").update(JSON.stringify({companyId:input.companyId,packageVersion:input.packageVersion,facts,events:events.results})).digest("hex"),outstanding:facts.filter(fact=>fact.pending).length};
}
