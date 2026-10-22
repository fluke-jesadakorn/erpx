import "server-only";
import {createHash} from "node:crypto";
import type {PostgresDatabase} from "@erpx/server-runtime/public/runtime/postgres";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {packageSettlementSource,type PackageSettlementSourceLock} from "@erpx/server-commercial/public/package-settlement-sources";
import {assertProjectPackageObligationSources,projectPackageSettlementSources as projects} from "@erpx/server-projects/public/package-obligations";
type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};const sources=new WeakMap<SqlDatabase,string>();
export async function lockProjectFinancePackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){const {database,companyId}=packageSettlementSource(owner,lock);await database.prepare("LOCK TABLE finance_journals,finance_journal_lines,finance_accounts,finance_charts IN SHARE MODE").run();sources.set(database,companyId);}
/** Allocations reference existing independently posted expense; they neither
 * own that journal nor turn unrelated Finance work into Project obligations. */
export async function projectFinancePackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 owner.assertActiveTransaction(database);if(sources.get(database)!==input.companyId)throw Error("PROJECT_FINANCE_PACKAGE_OBLIGATION_SOURCE_REQUIRED");assertProjectPackageObligationSources(owner,database,input.companyId);
 if(input.moduleId!=="finance"||!["projects","projects-costs"].includes(input.packageId)||input.packageVersion!==1||!Number.isSafeInteger(input.now)||input.now<0)throw Error("PROJECT_FINANCE_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
 const facts=(await database.prepare(`WITH scope AS(SELECT ?::text company_id)
 SELECT c.id,c.version,c.state,j.id journal_id,j.version journal_version,
 c.state='draft' OR p.id IS NULL OR p.company_id IS DISTINCT FROM c.company_id
 OR (c.state='approved' AND (l.id IS NULL OR j.id IS NULL OR j.company_id IS DISTINCT FROM c.company_id OR j.status<>'posted'
 OR account.id IS NULL OR account.chart_id IS DISTINCT FROM j.chart_id OR account.category NOT IN ('expense','other_expense')
 OR chart.company_id IS DISTINCT FROM c.company_id OR l.debit_minor<c.amount_minor OR l.credit_minor<>0
 OR c.approved_by IS NULL OR c.approved_by=c.created_by
 OR EXISTS(SELECT 1 FROM (${projects.actuals}) actual WHERE actual.source_kind='cost_allocation' AND actual.source_id=c.id
 AND (actual.company_id IS DISTINCT FROM c.company_id OR actual.project_id IS DISTINCT FROM c.project_id
 OR actual.source_version IS DISTINCT FROM c.version OR actual.amount_minor IS DISTINCT FROM c.amount_minor OR actual.currency IS DISTINCT FROM j.currency))
 OR (SELECT SUM(allocation.amount_minor) FROM (${projects.costs}) allocation WHERE allocation.journal_line_id=l.id AND allocation.state IN ('draft','approved'))>l.debit_minor
 OR (SELECT SUM(line.debit_minor)-SUM(line.credit_minor) FROM finance_journal_lines line WHERE line.journal_id=j.id)<>0)) pending
 FROM (${projects.costs}) c CROSS JOIN scope LEFT JOIN (${projects.projects}) p ON p.id=c.project_id
 LEFT JOIN finance_journal_lines l ON l.id=c.journal_line_id LEFT JOIN finance_journals j ON j.id=l.journal_id
 LEFT JOIN finance_accounts account ON account.id=l.account_id LEFT JOIN finance_charts chart ON chart.id=j.chart_id
 WHERE c.company_id=scope.company_id ORDER BY c.id`).bind(input.companyId).all<{id:string;version:number;state:string;journal_id:string|null;journal_version:number|null;pending:boolean}>()).results;
 return {companyId:input.companyId,packageId:input.packageId,moduleId:"finance",observedAt:input.now,revision:createHash("sha256").update(JSON.stringify({companyId:input.companyId,packageVersion:input.packageVersion,facts})).digest("hex"),outstanding:facts.filter(fact=>fact.pending).length};
}
