import {withFinancePurchasedCommand,withFinancePurchasedRead} from "@erpx/server-finance/adapters/purchase-boundary";
import {financeReportingCompanyFacts} from '@erpx/server-organization/public/finance-reporting-company-facts';
import { fiscalCloseSourceSql } from "@erpx/server-finance/public/finance-fiscal-close-source";
import "server-only";
import { z } from "zod";
import { createHash } from "node:crypto";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import { ApiError } from "@erpx/server-runtime/public/http";
import {prepareJournal} from '@erpx/server-finance/public/finance';
import { snapshotMoney } from "@erpx/server-finance/public/company-money";
import { createFinanceReporting } from "@erpx/server-finance/application/reporting";
import { statementPackageSchema, reportingDecisionSchema, yearClosingSchema, type Chart, type Profile, type Account, type Package } from "@erpx/server-finance/domain/reporting";
export { statementPackageSchema, reportingDecisionSchema, yearClosingSchema, type StatementSnapshot, type StatementSet } from "@erpx/server-finance/domain/reporting";
async function revision(db: SqlDatabase, chartId: string) { return Number(await db.prepare("SELECT revision FROM finance_reporting_revisions WHERE chart_id=?").bind(chartId).first("revision") ?? 0); }
function sourceGuards(db: SqlDatabase, actor: WorkContext, key: string, chartId: string, profileId: string, profileVersion: number, sourceRevision: number): SqlStatement[] {
    return [
        db.prepare("SELECT chart_id FROM finance_reporting_revisions WHERE chart_id=? FOR UPDATE").bind(chartId),
        assertion(db, key, "reporting-revision", "EXISTS(SELECT 1 FROM finance_reporting_revisions WHERE chart_id=? AND revision=?)", [chartId, sourceRevision]),
        assertion(db, key, "reporting-profile", "EXISTS(SELECT 1 FROM finance_accounting_profiles WHERE id=? AND company_id=? AND statutory_chart_id=? AND status='approved' AND version=?)", [profileId, actor.companyId, chartId, profileVersion]),
    ];
}
async function totals(db: SqlDatabase, actor: WorkContext, chartId: string, startDate: string, endDate: string) {
    return (await db.prepare(`WITH dates AS (SELECT ?::text starts,?::text ends)
  SELECT a.id,a.code,a.name,a.category,a.currency,a.status,
   COALESCE(SUM(l.base_debit_minor-l.base_credit_minor) FILTER(WHERE j.entry_date<=d.ends),0) balance_minor,
   COALESCE(SUM(l.base_debit_minor-l.base_credit_minor) FILTER(WHERE j.entry_date<d.starts),0) opening_minor,
   COALESCE(SUM(l.base_debit_minor-l.base_credit_minor) FILTER(WHERE j.entry_date BETWEEN d.starts AND d.ends AND NOT ${fiscalCloseSourceSql()}),0) income_minor,
   COALESCE(SUM(l.base_credit_minor-l.base_debit_minor) FILTER(WHERE j.entry_date BETWEEN d.starts AND d.ends AND NOT ${fiscalCloseSourceSql()} AND a.category='equity'),0) owner_movements_minor,
   COALESCE(SUM(l.base_credit_minor-l.base_debit_minor) FILTER(WHERE j.entry_date BETWEEN d.starts AND d.ends AND ${fiscalCloseSourceSql()} AND a.category='equity'),0) closing_transfer_minor
  FROM finance_accounts a CROSS JOIN dates d LEFT JOIN finance_journals j ON j.company_id=a.company_id AND j.chart_id=a.chart_id AND j.status IN ('posted','reversed') AND j.entry_date<=d.ends
  LEFT JOIN finance_journal_lines l ON l.journal_id=j.id AND l.account_id=a.id
  WHERE a.company_id=? AND a.chart_id=? GROUP BY a.id,a.code,a.name,a.category,a.currency,a.status ORDER BY a.code,a.id`)
        .bind(startDate, endDate, actor.companyId, chartId).all<Account>()).results;
}
async function sourceEvidence(db: SqlDatabase, actor: WorkContext, chartId: string, endDate: string) { return (await db.prepare("SELECT j.id,j.version,j.entry_date,j.source_type,j.source_id,j.currency,j.fx_rate_micros,l.id line_id,l.account_id,l.base_debit_minor,l.base_credit_minor,l.ordinal FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id WHERE j.company_id=? AND j.chart_id=? AND j.status IN ('posted','reversed') AND j.entry_date<=? ORDER BY j.id,l.ordinal,l.id").bind(actor.companyId, chartId, endDate).all()).results; }
export async function reportingWorkspace(actor: WorkContext, chartId: string) { return withFinancePurchasedRead(actor,db=>nativeReporting(db).reportingWorkspace(actor,chartId)); }
export async function statementPackageDetail(actor: WorkContext, id: string) { return withFinancePurchasedRead(actor,db=>nativeReporting(db).statementPackageDetail(actor,id)); }
export async function prepareStatementPackage(actor: WorkContext, input: z.input<typeof statementPackageSchema>, key: string) { return withFinancePurchasedCommand(actor,{key,operation:"finance.statement.prepare",purpose:"settle-existing"},tx=>nativeReporting(tx).prepareStatementPackage(actor,input,key)); }
export async function decideStatementPackage(actor: WorkContext, id: string, action: "issue" | "reject" | "cancel", input: z.input<typeof reportingDecisionSchema>, key: string) { return withFinancePurchasedCommand(actor,{key,operation:`finance.statement.${action}`,purpose:"settle-existing"},tx=>nativeReporting(tx).decideStatementPackage(actor,id,action,input,key)); }
export async function statementPackageExport(actor: WorkContext, id: string) { return withFinancePurchasedRead(actor,db=>nativeReporting(db).statementPackageExport(actor,id)); }
export async function prepareYearClosing(actor: WorkContext, input: z.input<typeof yearClosingSchema>, key: string) { return withFinancePurchasedCommand(actor,{key,operation:"finance.year-close.prepare",purpose:"settle-existing",capability:"finance.close"},tx=>nativeReporting(tx).prepareYearClosing(actor,input,key)); }
export async function decideYearClosing(actor: WorkContext, id: string, action: "post" | "reject" | "cancel", input: z.input<typeof reportingDecisionSchema>, key: string) { return withFinancePurchasedCommand(actor,{key,operation:`finance.year-close.${action}`,purpose:"settle-existing",capability:"finance.close"},tx=>nativeReporting(tx).decideYearClosing(actor,id,action,input,key)); }
export async function statementSourceAccounts(actor: WorkContext, id: string, args: Record<string, string>, page = 1) { return withFinancePurchasedRead(actor,db=>nativeReporting(db).statementSourceAccounts(actor,id,args,page)); }
function nativeReporting(provided?:SqlDatabase) {
    let pending: Promise<SqlDatabase> | undefined = provided?Promise.resolve(provided):undefined, preparedMoney: Awaited<ReturnType<typeof snapshotMoney>> | null = null;
    const journals = new Map<string, Awaited<ReturnType<typeof prepareJournal>>>(), database = () => pending!;
    return createFinanceReporting({ error: (...args) => new ApiError(...args), permission: requirePermission, ready: async (actor) => { await (pending ??= holdingResources(actor.holdingId).then(r => r.write)); }, now: Date.now, hash: bytes => createHash("sha256").update(bytes, "utf8").digest("hex"), replay: async <Result>(actor:WorkContext,key:string,op:string,id:string,input:unknown) => replayCommand<Result>(await database(), actor, key, op, id, input), chart: async (actor, chartId) => (await database()).prepare(`SELECT c.id,c.name,c.base_currency,c.status,${financeReportingCompanyFacts().companyName} FROM finance_charts c ${financeReportingCompanyFacts().companyJoin} WHERE c.id=? AND c.company_id=?`).bind(chartId, actor.companyId).first<Chart>(), profile: async (actor, chartId, startDate) => (await database()).prepare("SELECT * FROM finance_accounting_profiles WHERE company_id=? AND statutory_chart_id=? AND status='approved' AND standard='TFRS for NPAEs' AND effective_date<=?").bind(actor.companyId, chartId, startDate).first<Profile>(), revision: async (chart) => revision(await database(), chart), totals: async (actor, chart, start, end) => totals(await database(), actor, chart, start, end), evidence: async (actor, chart, end) => sourceEvidence(await database(), actor, chart, end), chartExists: async (actor, chart) => !!await (await database()).prepare("SELECT id FROM finance_charts WHERE id=? AND company_id=?").bind(chart, actor.companyId).first(), packages: async (actor, chart) => (await (await database()).prepare("SELECT id,start_date,end_date,comparative_start_date,comparative_end_date,edition,previous_package_id,status,version,prepared_by,reviewed_by,source_fingerprint,created_at FROM finance_statement_packages WHERE company_id=? AND chart_id=? ORDER BY end_date DESC,edition DESC").bind(actor.companyId, chart).all<Package>()).results, closings: async (actor, chart) => (await (await database()).prepare("SELECT * FROM finance_year_closings WHERE company_id=? AND chart_id=? ORDER BY end_date DESC,created_at DESC").bind(actor.companyId, chart).all<Package>()).results, accounts: async (actor, chart) => (await (await database()).prepare("SELECT id,code,name,category,status FROM finance_accounts WHERE company_id=? AND chart_id=? ORDER BY code,id").bind(actor.companyId, chart).all<Package>()).results, package: async (actor, id) => (await database()).prepare("SELECT * FROM finance_statement_packages WHERE id=? AND company_id=?").bind(id, actor.companyId).first<Package>(), statementHistory: async (id) => (await (await database()).prepare("SELECT * FROM finance_statement_events WHERE package_id=? ORDER BY package_version").bind(id).all()).results, previousPackage: async (actor, input) => (await database()).prepare("SELECT id FROM finance_statement_packages WHERE id=? AND company_id=? AND chart_id=? AND start_date=? AND end_date=? AND status='issued'").bind(input.previousPackageId, actor.companyId, input.chartId, input.startDate, input.endDate).first(), nextEdition: async (input) => Number(await (await database()).prepare("SELECT COALESCE(MAX(edition),0)+1 edition FROM finance_statement_packages WHERE chart_id=? AND start_date=? AND end_date=?").bind(input.chartId, input.startDate, input.endDate).first("edition")), existingClosing: async (actor, input) => !!await (await database()).prepare("SELECT id FROM finance_year_closings WHERE company_id=? AND chart_id=? AND start_date=? AND end_date=? AND status IN ('draft','posted')").bind(actor.companyId, input.chartId, input.startDate, input.endDate).first(), period: async (actor, input) => (await database()).prepare("SELECT id,start_date,end_date,state FROM finance_periods WHERE id=? AND company_id=?").bind(input.periodId, actor.companyId).first<{
            id: string;
            start_date: string;
            end_date: string;
            state: string;
        }>(), equityAccount: async (actor, input, currency) => !!await (await database()).prepare("SELECT id FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=? AND category='equity' AND status='active' AND currency=?").bind(input.retainedEarningsAccountId, actor.companyId, input.chartId, currency).first(), closing: async (actor, id) => (await database()).prepare("SELECT * FROM finance_year_closings WHERE id=? AND company_id=?").bind(id, actor.companyId).first<Package>(), prepareJournal: async (actor, input, key) => { const p = await prepareJournal(await database(), actor, input, key, "posted"); journals.set(p.result.id, p); return { result: { id: p.result.id } }; }, prepareMoney: async (actor, input) => { preparedMoney = await snapshotMoney(await database(), actor, input); },
        commitStatement: async ({ actor, input, key, id, op, profile, sourceRevision, sourceFingerprint, snapshot, edition, now }) => {
            const write = await database();
            return executeCommand(write, actor, key, op, input, id, 1, { id, status: "draft", version: 1, edition, sourceFingerprint }, [
                ...sourceGuards(write, actor, key, input.chartId, profile.id, profile.version, sourceRevision),
                assertion(write, key, "statement-edition", "NOT EXISTS(SELECT 1 FROM finance_statement_packages WHERE chart_id=? AND start_date=? AND end_date=? AND edition=?)", [input.chartId, input.startDate, input.endDate, edition]),
                write.prepare("INSERT INTO finance_statement_packages(id,company_id,chart_id,profile_id,profile_version,start_date,end_date,comparative_start_date,comparative_end_date,previous_package_id,edition,source_revision,source_fingerprint,presentation_json,notes_json,snapshot_json,reason,prepared_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(id, actor.companyId, input.chartId, profile.id, profile.version, input.startDate, input.endDate, input.comparativeStartDate, input.comparativeEndDate, input.previousPackageId ?? null, edition, sourceRevision, sourceFingerprint, JSON.stringify(input.presentation), JSON.stringify(input.notes), JSON.stringify(snapshot), input.reason, actor.userId, now, now),
                write.prepare("INSERT INTO finance_statement_events VALUES(?,?,?,'prepared',?,1,?,?,?)").bind(`statement-event-${key}`, id, actor.companyId, actor.userId, input.reason, key, now),
            ]);
        }, commitStatementDecision: async ({ actor, id, action, input, key, op, row, html, sha256, now, status, next }) => {
            const write = await database();
            return executeCommand(write, actor, key, op, input, id, next, { id, status, version: next, edition: Number(row.edition), sha256 }, [
                ...(action === "issue" ? sourceGuards(write, actor, key, String(row.chart_id), String(row.profile_id), Number(row.profile_version), Number(row.source_revision)) : []),
                assertion(write, key, "statement-version", "EXISTS(SELECT 1 FROM finance_statement_packages WHERE id=? AND company_id=? AND status='draft' AND version=?)", [id, actor.companyId, input.expectedVersion]),
                write.prepare("UPDATE finance_statement_packages SET status=?,reviewed_by=?,review_reason=?,reviewed_at=?,export_html=?,export_sha256=?,version=version+1,updated_at=? WHERE id=?").bind(status, actor.userId, input.reason, now, html, sha256, now, id),
                write.prepare("INSERT INTO finance_statement_events VALUES(?,?,?,?,?,?,?,?,?)").bind(`statement-event-${key}`, id, actor.companyId, status, actor.userId, next, input.reason, key, now),
            ]);
        }, commitYearClosing: async ({ actor, input, key, id, op, profile, sourceRevision, sourceFingerprint, snapshot, profit, now }) => {
            const write = await database();
            return executeCommand(write, actor, key, op, input, id, 1, { id, status: "draft", version: 1, profitMinor: profit, sourceFingerprint }, [
                ...sourceGuards(write, actor, key, input.chartId, profile.id, profile.version, sourceRevision),
                assertion(write, key, "earlier-periods", "NOT EXISTS(SELECT 1 FROM finance_periods WHERE company_id=? AND id<>? AND start_date<=? AND end_date>=? AND state<>'closed')", [actor.companyId, input.periodId, input.endDate, input.startDate]),
                assertion(write, key, "year-drafts", "NOT EXISTS(SELECT 1 FROM finance_journals WHERE company_id=? AND chart_id=? AND entry_date BETWEEN ? AND ? AND status IN ('draft','in_review'))", [actor.companyId, input.chartId, input.startDate, input.endDate]),
                write.prepare("INSERT INTO finance_year_closings(id,company_id,chart_id,profile_id,profile_version,start_date,end_date,period_id,retained_earnings_account_id,source_revision,source_fingerprint,snapshot_json,profit_minor,reason,prepared_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(id, actor.companyId, input.chartId, profile.id, profile.version, input.startDate, input.endDate, input.periodId, input.retainedEarningsAccountId, sourceRevision, sourceFingerprint, JSON.stringify(snapshot), profit, input.reason, actor.userId, now, now),
                write.prepare("INSERT INTO finance_year_closing_events VALUES(?,?,?,'prepared',?,1,?,?,?)").bind(`year-closing-event-${key}`, id, actor.companyId, actor.userId, input.reason, key, now),
            ]);
        }, commitClosingDecision: async ({ actor, id, action, input, key, op, row, now, status, next, journalId }) => {
            const write = await database();
            const journal = journalId ? journals.get(journalId)! : null, money = preparedMoney;
            return executeCommand(write, actor, key, op, input, id, next, { id, status, version: next, profitMinor: Number(row.profit_minor), journalId: journal?.result.id ?? null }, [
                ...(action === "post" ? sourceGuards(write, actor, key, String(row.chart_id), String(row.profile_id), Number(row.profile_version), Number(row.source_revision)) : []),
                assertion(write, key, "closing-version", "EXISTS(SELECT 1 FROM finance_year_closings WHERE id=? AND company_id=? AND status='draft' AND version=?)", [id, actor.companyId, input.expectedVersion]),
                ...(action === "post" ? [assertion(write, key, "closing-period", "EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND end_date=?)", [row.period_id, actor.companyId, row.end_date]), assertion(write, key, "earlier-periods", "NOT EXISTS(SELECT 1 FROM finance_periods WHERE company_id=? AND id<>? AND start_date<=? AND end_date>=? AND state<>'closed')", [actor.companyId, row.period_id, row.end_date, row.start_date])] : []),
                ...(journal?.steps ?? []), ...(money ? [money.statement] : []),
                write.prepare("UPDATE finance_year_closings SET status=?,journal_id=?,reviewed_by=?,review_reason=?,reviewed_at=?,version=version+1,updated_at=? WHERE id=?").bind(status, journal?.result.id ?? null, actor.userId, input.reason, now, now, id),
                write.prepare("INSERT INTO finance_year_closing_events VALUES(?,?,?,?,?,?,?,?,?)").bind(`year-closing-event-${key}`, id, actor.companyId, status, actor.userId, next, input.reason, key, now),
            ]);
        }
    });
}
