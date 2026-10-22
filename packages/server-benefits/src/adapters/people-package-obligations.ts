import "server-only";
import {createHash} from "node:crypto";
import type {PostgresDatabase} from "@erpx/server-runtime/public/runtime/postgres";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {packageSettlementSource,type PackageSettlementSourceLock} from "@erpx/server-commercial/public/package-settlement-sources";
import {assertPeoplePackageObligationSources,peoplePackageSettlementSources as people} from "@erpx/server-people_payroll/public/package-obligations";
type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};const sources=new WeakMap<SqlDatabase,string>();
export async function lockPeopleBenefitsPackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){const {database,companyId}=packageSettlementSource(owner,lock);await database.prepare("LOCK TABLE benefit_plans,benefit_plan_events,benefit_dependents,benefit_dependent_events,benefit_enrollment_windows,benefit_window_events,benefit_elections,benefit_election_events,benefit_payroll_inputs IN SHARE MODE").run();sources.set(database,companyId);}
/** Benefits owns reviewed elections and immutable Payroll deliveries. Its
 * schema has no Roster/Time/Leave/Overtime source FK: unrelated elections cannot
 * block the Workforce add-on. Actual rows still participate in the revision.
 * Base People retains pending review and future monetary coverage duties. */
export async function peopleBenefitsPackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 owner.assertActiveTransaction(database);if(sources.get(database)!==input.companyId)throw Error("PEOPLE_BENEFITS_PACKAGE_OBLIGATION_SOURCE_REQUIRED");assertPeoplePackageObligationSources(owner,database,input.companyId);
 if(input.moduleId!=="benefits"||!["people","people-workforce"].includes(input.packageId)||input.packageVersion!==1||!Number.isSafeInteger(input.now)||input.now<0)throw Error("PEOPLE_BENEFITS_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
 const facts=(await database.prepare(`WITH scope AS(SELECT ?::text company_id,?::boolean is_base,to_char(to_timestamp(?::bigint/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD') today),
 facts AS(
 SELECT 'plan' kind,p.id,p.version,p.status state,to_jsonb(p) image,p.status='draft' pending FROM benefit_plans p,scope WHERE p.company_id=scope.company_id
 UNION ALL SELECT 'window',w.id,w.version,w.status,to_jsonb(w),w.status IN ('draft','open') FROM benefit_enrollment_windows w,scope WHERE w.company_id=scope.company_id
 UNION ALL SELECT 'dependent',d.id,d.version,d.status,to_jsonb(d),d.status='pending' OR NOT EXISTS(SELECT 1 FROM (${people.people}) p WHERE p.id=d.person_id AND p.company_id=d.company_id) FROM benefit_dependents d,scope WHERE d.company_id=scope.company_id
 UNION ALL SELECT 'election',e.id,e.version,e.status,to_jsonb(e),e.status NOT IN ('released','cancelled') OR p.id IS NULL OR engagement.id IS NULL OR engagement.person_id IS DISTINCT FROM e.person_id OR plan.id IS NULL OR plan.company_id IS DISTINCT FROM e.company_id OR plan.version<e.plan_version OR win.id IS NULL OR win.company_id IS DISTINCT FROM e.company_id OR win.version<e.window_version
 OR (e.status='released' AND (e.released_by IS NULL OR NOT EXISTS(SELECT 1 FROM benefit_payroll_inputs handoff WHERE handoff.election_id=e.id AND handoff.company_id=e.company_id AND handoff.person_id=e.person_id AND handoff.engagement_id=e.engagement_id AND handoff.currency=e.currency AND handoff.source_election_version<=e.version AND handoff.employee_deduction_minor=e.employee_deduction_minor AND handoff.employer_cost_minor=e.employer_cost_minor AND handoff.effective_from=e.effective_from AND handoff.effective_to IS NOT DISTINCT FROM e.effective_to AND EXISTS(SELECT 1 FROM benefit_election_events event WHERE event.election_id=e.id AND event.version=handoff.source_election_version AND event.action='release' AND event.status='released' AND event.actor_id=handoff.released_by))
 OR (e.employee_deduction_minor+e.employer_cost_minor>0 AND (e.effective_to IS NULL OR e.effective_to>=scope.today))))
 FROM benefit_elections e CROSS JOIN scope LEFT JOIN (${people.people}) p ON p.id=e.person_id AND p.company_id=e.company_id LEFT JOIN (${people.engagements}) engagement ON engagement.id=e.engagement_id AND engagement.company_id=e.company_id LEFT JOIN benefit_plans plan ON plan.id=e.plan_id LEFT JOIN benefit_enrollment_windows win ON win.id=e.window_id WHERE e.company_id=scope.company_id
 UNION ALL SELECT 'payroll-input',h.id,h.source_election_version,h.status,to_jsonb(h),e.id IS NULL OR e.status<>'released' OR e.company_id IS DISTINCT FROM h.company_id OR e.person_id IS DISTINCT FROM h.person_id OR e.engagement_id IS DISTINCT FROM h.engagement_id OR e.version<h.source_election_version OR e.currency IS DISTINCT FROM h.currency OR e.employee_deduction_minor IS DISTINCT FROM h.employee_deduction_minor OR e.employer_cost_minor IS DISTINCT FROM h.employer_cost_minor
 FROM benefit_payroll_inputs h CROSS JOIN scope LEFT JOIN benefit_elections e ON e.id=h.election_id WHERE h.company_id=scope.company_id
 ) SELECT kind,id,version,state,image,scope.is_base AND COALESCE(facts.pending,true) pending FROM facts CROSS JOIN scope ORDER BY kind,id`).bind(input.companyId,input.packageId==='people',input.now).all<{kind:string;id:string;version:number;state:string;image:unknown;pending:boolean}>()).results;
 return {companyId:input.companyId,packageId:input.packageId,moduleId:"benefits",observedAt:input.now,revision:createHash("sha256").update(JSON.stringify({companyId:input.companyId,packageId:input.packageId,packageVersion:input.packageVersion,facts})).digest("hex"),outstanding:facts.filter(fact=>fact.pending).length};
}
