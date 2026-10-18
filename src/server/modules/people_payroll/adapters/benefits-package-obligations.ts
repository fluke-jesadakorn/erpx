import "server-only";
import {createHash} from "node:crypto";
import type {PostgresDatabase} from "../../runtime/public/runtime/postgres";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {packageSettlementSource,type PackageSettlementSourceLock} from "../../commercial/public/package-settlement-sources";
import {assertBenefitPackageObligationSources,benefitPackageSettlementSources as benefits} from "../../benefits/public/package-obligations";

type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};
const sources=new WeakMap<SqlDatabase,string>();
export async function lockBenefitPeoplePackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){const {database,companyId}=packageSettlementSource(owner,lock);await database.prepare("LOCK TABLE hr_people,hr_engagements,hr_payroll_runs,hr_payroll_items,hr_payroll_postings IN SHARE MODE").run();sources.set(database,companyId);}
export function assertBenefitPeoplePackageObligationSources(owner:PostgresDatabase,database:SqlDatabase,companyId:string){owner.assertActiveTransaction(database);if(sources.get(database)!==companyId)throw Error("BENEFIT_PEOPLE_PACKAGE_OBLIGATION_SOURCE_REQUIRED");assertBenefitPackageObligationSources(owner,database,companyId);}
/** A later Payroll calculation retains its original owner. Only actual captured
 * Benefits input IDs join that owner; unrelated payroll never becomes an addon duty. */
export const benefitPayrollSettlementSources={
 captures:`SELECT item.id,item.run_id,item.engagement_id,item.status,item.input_snapshot_json,run.company_id,run.version run_version,run.status run_status,run.currency run_currency,benefit.value source,
 input.id input_id,input.election_id,input.company_id input_company_id,input.person_id,input.engagement_id input_engagement_id,input.currency input_currency,input.source_election_version,input.employee_deduction_minor,input.employer_cost_minor
 FROM hr_payroll_items item JOIN hr_payroll_runs run ON run.id=item.run_id
 CROSS JOIN LATERAL jsonb_array_elements(CASE WHEN jsonb_typeof(item.input_snapshot_json::jsonb->'benefitInputs')='array' THEN item.input_snapshot_json::jsonb->'benefitInputs' ELSE '[]'::jsonb END) benefit(value)
 JOIN (${benefits.inputs}) input ON input.id=benefit.value->>'id' AND input.company_id=run.company_id`,
 runs:"SELECT * FROM hr_payroll_runs",postings:"SELECT * FROM hr_payroll_postings",
} as const;
export async function benefitPeoplePackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 assertBenefitPeoplePackageObligationSources(owner,database,input.companyId);
 if(input.moduleId!=="people_payroll"||input.packageId!=="people-benefits"||input.packageVersion!==1||!Number.isSafeInteger(input.now)||input.now<0)throw Error("BENEFIT_PEOPLE_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
 const facts=(await database.prepare(`WITH scope AS(SELECT ?::text company_id),elections AS MATERIALIZED(SELECT e.* FROM (${benefits.elections}) e,scope WHERE e.company_id=scope.company_id)
 SELECT 'worker' kind,e.id,e.version,e.status state,jsonb_build_object('election',to_jsonb(e),'person',jsonb_build_object('id',p.id,'companyId',p.company_id,'status',p.status,'version',p.version,'userId',p.user_id),'engagement',to_jsonb(engagement)) image,
 p.id IS NULL OR engagement.id IS NULL OR engagement.person_id IS DISTINCT FROM e.person_id OR NOT e.terminal_proven pending
 FROM elections e LEFT JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id LEFT JOIN hr_engagements engagement ON engagement.id=e.engagement_id AND engagement.company_id=e.company_id
 UNION ALL SELECT 'dependent-worker',d.id,d.version,d.status,to_jsonb(d)||jsonb_build_object('personId',p.id,'personCompanyId',p.company_id,'personVersion',p.version),p.id IS NULL
 FROM (${benefits.dependents}) d CROSS JOIN scope LEFT JOIN hr_people p ON p.id=d.person_id AND p.company_id=d.company_id WHERE d.company_id=scope.company_id
 UNION ALL SELECT 'payroll-capture',capture.id,capture.run_version,capture.run_status,to_jsonb(capture),capture.input_company_id IS DISTINCT FROM capture.company_id OR capture.engagement_id IS DISTINCT FROM capture.input_engagement_id OR capture.run_currency IS DISTINCT FROM capture.input_currency
 OR capture.source->>'election_id' IS DISTINCT FROM capture.election_id OR capture.source->>'person_id' IS DISTINCT FROM capture.person_id OR capture.source->>'engagement_id' IS DISTINCT FROM capture.input_engagement_id OR capture.source->>'currency' IS DISTINCT FROM capture.input_currency OR capture.source->>'source_election_version' IS DISTINCT FROM capture.source_election_version::text OR capture.source->>'employee_deduction_minor' IS DISTINCT FROM capture.employee_deduction_minor::text OR capture.source->>'employer_cost_minor' IS DISTINCT FROM capture.employer_cost_minor::text
 FROM (${benefitPayrollSettlementSources.captures}) capture,scope WHERE capture.company_id=scope.company_id
 ORDER BY kind,id`).bind(input.companyId).all<{kind:string;id:string;version:number;state:string;image:unknown;pending:boolean}>()).results;
 return {companyId:input.companyId,packageId:input.packageId,moduleId:"people_payroll",observedAt:input.now,revision:createHash("sha256").update(JSON.stringify({companyId:input.companyId,packageVersion:input.packageVersion,facts})).digest("hex"),outstanding:facts.filter(fact=>fact.pending).length};
}
