import "server-only";
import {createHash} from "node:crypto";
import type {PostgresDatabase} from "@erpx/server-runtime/public/runtime/postgres";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {packageSettlementSource,type PackageSettlementSourceLock} from "@erpx/server-commercial/public/package-settlement-sources";
import {assertBenefitPeoplePackageObligationSources,benefitPayrollSettlementSources as people} from "@erpx/server-people_payroll/public/benefits-package-obligations";
type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};
const sources=new WeakMap<SqlDatabase,string>();
export async function lockBenefitFinancePackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){const {database,companyId}=packageSettlementSource(owner,lock);await database.prepare("LOCK TABLE finance_journals,finance_journal_lines,finance_charts,finance_book_postings,finance_book_rules IN SHARE MODE").run();sources.set(database,companyId);}
/** Benefits delivers its exact immutable input to Payroll. It neither owns
 * unrelated payroll/payments nor requires a new Payroll or Finance purchase. */
export async function benefitFinancePackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 owner.assertActiveTransaction(database);if(sources.get(database)!==input.companyId)throw Error("BENEFIT_FINANCE_PACKAGE_OBLIGATION_SOURCE_REQUIRED");assertBenefitPeoplePackageObligationSources(owner,database,input.companyId);
 if(input.moduleId!=="finance"||input.packageId!=="people-benefits"||input.packageVersion!==1||!Number.isSafeInteger(input.now)||input.now<0)throw Error("BENEFIT_FINANCE_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
 const facts=(await database.prepare(`WITH scope AS(SELECT ?::text company_id),runs AS MATERIALIZED(SELECT DISTINCT r.* FROM (${people.runs}) r JOIN (${people.captures}) capture ON capture.run_id=r.id,scope WHERE r.company_id=scope.company_id),
 journals AS MATERIALIZED(SELECT j.*,COALESCE((SELECT SUM(line.debit_minor-line.credit_minor) FROM finance_journal_lines line WHERE line.journal_id=j.id),0) imbalance,(SELECT count(*) FROM finance_journal_lines line WHERE line.journal_id=j.id) line_count FROM finance_journals j),
 owned AS MATERIALIZED(SELECT p.* FROM (${people.postings}) p JOIN runs r ON r.id=p.run_id),facts AS(
 SELECT 'posted-payroll' kind,r.id,r.version,r.status state,jsonb_build_object('run',to_jsonb(r),'posting',to_jsonb(p),'journal',to_jsonb(j)) image,
 (r.status='posted' AND (p.id IS NULL OR j.id IS NULL OR j.status<>'posted' OR j.company_id IS DISTINCT FROM r.company_id OR j.currency IS DISTINCT FROM r.currency OR j.source_type<>'payroll' OR j.source_id IS DISTINCT FROM r.id OR j.imbalance<>0 OR j.line_count<2)) OR (p.id IS NOT NULL AND (r.status<>'posted' OR j.id IS NULL OR j.status<>'posted' OR j.company_id IS DISTINCT FROM r.company_id OR j.source_type<>'payroll' OR j.source_id IS DISTINCT FROM r.id OR j.imbalance<>0 OR j.line_count<2)) pending
 FROM runs r LEFT JOIN owned p ON p.run_id=r.id LEFT JOIN journals j ON j.id=p.journal_id
 UNION ALL SELECT 'secondary-journal',book.secondary_journal_id,book.source_version,'posted',jsonb_build_object('posting',to_jsonb(book),'journal',to_jsonb(secondary),'rule',to_jsonb(rule)),book.company_id IS DISTINCT FROM r.company_id OR secondary.id IS NULL OR secondary.status<>'posted' OR secondary.company_id IS DISTINCT FROM r.company_id OR secondary.imbalance<>0 OR secondary.line_count<2 OR secondary.currency IS DISTINCT FROM r.currency OR rule.id IS NULL OR rule.status<>'approved' OR rule.reviewed_by IS NULL OR rule.reviewed_by=rule.prepared_by
 FROM owned p JOIN runs r ON r.id=p.run_id JOIN finance_book_postings book ON book.primary_journal_id=p.journal_id LEFT JOIN journals secondary ON secondary.id=book.secondary_journal_id LEFT JOIN finance_book_rules rule ON rule.id=book.rule_id
 ) SELECT kind,id,version,state,image,COALESCE(pending,true) pending FROM facts ORDER BY kind,id`).bind(input.companyId).all<{kind:string;id:string;version:number;state:string;image:unknown;pending:boolean}>()).results;
 return {companyId:input.companyId,packageId:input.packageId,moduleId:"finance",observedAt:input.now,revision:createHash("sha256").update(JSON.stringify({companyId:input.companyId,packageVersion:input.packageVersion,facts})).digest("hex"),outstanding:facts.filter(fact=>fact.pending).length};
}
