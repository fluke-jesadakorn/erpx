import { z } from 'zod';
import { samePerson } from "@/lib/identity_access/iam";
import { minorToDecimal } from "@/lib/finance/finance-money";
import { financialPresentation, type FinancialPresentationCode } from "@/lib/finance/finance-reporting";
import type { WorkContext } from '@/server/modules/identity_access/public/authorization';
import type { FinanceError } from '../domain/invoice-line-revision';
import type { JournalInput, JournalLineInput } from '../domain/finance';
import type { SnapshotInput } from '../domain/company-money';
import { statementPackageSchema, reportingDecisionSchema, yearClosingSchema, createReportingPolicy, statementHtml, fiscalDates, type Chart, type Profile, type Account, type Package, type Presentation, type StatementSnapshot, type StatementInput, type ReportingDecision, type YearClosingInput, type ReportingRow } from '../domain/reporting';
type ReportingCommand<Input> = {
    actor: WorkContext;
    input: Input;
    key: string;
    id: string;
    op: string;
    now: number;
};
export interface FinanceReportingPorts {
    error: FinanceError;
    permission(actor: WorkContext, permission: 'finance.read' | 'finance.write' | 'finance.approve'): WorkContext;
    ready(actor: WorkContext): Promise<void>;
    now(): number;
    hash(bytes: string): string;
    replay<Result=Record<string,unknown>>(actor: WorkContext, key: string, op: string, id: string, input: unknown): Promise<Result|null>;
    chart(actor: WorkContext, id: string): Promise<Chart | null>;
    profile(actor: WorkContext, chartId: string, startDate: string): Promise<Profile | null>;
    revision(chartId: string): Promise<number>;
    totals(actor: WorkContext, chartId: string, start: string, end: string): Promise<Account[]>;
    evidence(actor: WorkContext, chartId: string, end: string): Promise<ReportingRow[]>;
    chartExists(actor: WorkContext, id: string): Promise<boolean>;
    packages(actor: WorkContext, chartId: string): Promise<Package[]>;
    closings(actor: WorkContext, chartId: string): Promise<Package[]>;
    accounts(actor: WorkContext, chartId: string): Promise<Package[]>;
    package(actor: WorkContext, id: string): Promise<Package | null>;
    statementHistory(id: string): Promise<ReportingRow[]>;
    previousPackage(actor: WorkContext, input: StatementInput): Promise<unknown | null>;
    nextEdition(input: StatementInput): Promise<number>;
    existingClosing(actor: WorkContext, input: YearClosingInput): Promise<boolean>;
    period(actor: WorkContext, input: YearClosingInput): Promise<{
        id: string;
        start_date: string;
        end_date: string;
        state: string;
    } | null>;
    equityAccount(actor: WorkContext, input: YearClosingInput, currency: string): Promise<boolean>;
    closing(actor: WorkContext, id: string): Promise<Package | null>;
    prepareJournal(actor: WorkContext, input: JournalInput, key: string): Promise<{
        result: {
            id: string;
        };
    }>;
    prepareMoney(actor: WorkContext, input: SnapshotInput): Promise<void>;
    commitStatement(facts: ReportingCommand<StatementInput> & {
        profile: Profile;
        sourceRevision: number;
        sourceFingerprint: string;
        snapshot: StatementSnapshot;
        edition: number;
    }): Promise<CommitStatementResult>;
    commitStatementDecision(facts: ReportingCommand<ReportingDecision> & {
        action: 'issue' | 'reject' | 'cancel';
        row: Package;
        html: string | null;
        sha256: string | null;
        status: string;
        next: number;
    }): Promise<CommitStatementDecisionResult>;
    commitYearClosing(facts: ReportingCommand<YearClosingInput> & {
        profile: Profile;
        sourceRevision: number;
        sourceFingerprint: string;
        snapshot: {
            chart: Chart;
            profile: Profile;
            accounts: Account[];
            profitMinor: number;
            sourceFingerprint: string;
        };
        profit: number;
    }): Promise<CommitYearClosingResult>;
    commitClosingDecision(facts: ReportingCommand<ReportingDecision> & {
        action: 'post' | 'reject' | 'cancel';
        row: Package;
        status: string;
        next: number;
        journalId: string | null;
    }): Promise<CommitClosingDecisionResult>;
}
export function createFinanceReporting(ports: FinanceReportingPorts) {
    const { amount, statement } = createReportingPolicy(ports.error), digest = (value: unknown) => ports.hash(JSON.stringify(value));
    function companyScope(actor: WorkContext) { if (actor.branchId || actor.departmentId)
        throw ports.error(403, "COMPANY_FINANCE_REQUIRED", "Use a Company Finance assignment for statements and fiscal closing."); }
    async function chartAndProfile(actor: WorkContext, chartId: string, startDate: string) {
        const chart = await ports.chart(actor, chartId);
        if (!chart)
            throw ports.error(404, "CHART_NOT_FOUND", "Company reporting ledger not found.");
        const profile = await ports.profile(actor, chartId, startDate);
        if (!profile)
            throw ports.error(409, "NPAES_PROFILE_REQUIRED", "Review the Company's statutory ledger and accounting policies before preparing NPAEs statements.");
        return { chart, profile };
    }
    async function reportingWorkspace(actor: WorkContext, chartId: string) {
        actor = ports.permission(actor, "finance.read");
        companyScope(actor);
        await ports.ready(actor);
        if (!await ports.chartExists(actor, chartId))
            throw ports.error(404, "CHART_NOT_FOUND", "Company ledger not found.");
        const [packages, closings, accounts] = await Promise.all([ports.packages(actor, chartId), ports.closings(actor, chartId), ports.accounts(actor, chartId)]);
        return { packages, closings, accounts };
    }
    async function statementPackageDetail(actor: WorkContext, id: string) { actor = ports.permission(actor, "finance.read"); companyScope(actor); await ports.ready(actor); const row = await ports.package(actor, id); if (!row)
        throw ports.error(404, "STATEMENT_PACKAGE_NOT_FOUND", "Statement package not found."); const history = await ports.statementHistory(id); return { row, snapshot: JSON.parse(String(row.snapshot_json)) as StatementSnapshot, history }; }
    async function prepareStatementPackage(actor: WorkContext, raw: z.input<typeof statementPackageSchema>, key: string) {
        actor = ports.permission(actor, "finance.write");
        companyScope(actor);
        const input = statementPackageSchema.parse(raw);
        await ports.ready(actor);
        const id = `statement-package-${key}`, op = "finance.statement.prepare", prior = await ports.replay<CommitStatementResult>(actor, key, op, id, input);
        if (prior)
            return prior;
        if (input.startDate > input.endDate || input.comparativeStartDate > input.comparativeEndDate || input.comparativeEndDate >= input.startDate)
            throw ports.error(422, "STATEMENT_DATES_INVALID", "Current and preceding comparative dates must be ordered and separate.");
        if (new Set(input.presentation.map(row => row.accountId)).size !== input.presentation.length)
            throw ports.error(422, "STATEMENT_PRESENTATION_DUPLICATE", "Classify each ledger account once.");
        const { chart, profile } = await chartAndProfile(actor, input.chartId, input.startDate), sourceRevision = await ports.revision(input.chartId);
        const [current, comparative, evidence] = await Promise.all([ports.totals(actor, input.chartId, input.startDate, input.endDate), ports.totals(actor, input.chartId, input.comparativeStartDate, input.comparativeEndDate), ports.evidence(actor, input.chartId, input.endDate)]);
        const allowed = new Set(current.map(account => account.id));
        if (input.presentation.some(row => !allowed.has(row.accountId)))
            throw ports.error(422, "STATEMENT_PRESENTATION_INVALID", "Every classified account must belong to this Company ledger.");
        const sourceFingerprint = digest({ chart, profile, accounts: current, evidence }), snapshot: StatementSnapshot = { standard: "TFRS for NPAEs (revised 2022)", certified: false, company: chart.company_name, chart, profile, current: statement(current, input.presentation, input.startDate, input.endDate), comparative: statement(comparative, input.presentation, input.comparativeStartDate, input.comparativeEndDate), notes: input.notes, sourceFingerprint, sourceRevision, standardsSource: { url: "https://acpro-std.tfac.or.th/standard/2/-NPAEs", effectiveFrom: "2023-01-01", verifiedOn: "2026-10-03" } };
        if (await ports.revision(input.chartId) !== sourceRevision)
            throw ports.error(409, "REPORTING_SOURCE_CHANGED", "Ledger data changed while preparing the statement. Refresh and prepare again.");
        const previous = input.previousPackageId ? await ports.previousPackage(actor, input) : null;
        if (input.previousPackageId && !previous)
            throw ports.error(409, "STATEMENT_PREVIOUS_INVALID", "An amended edition must reference the issued statement for the same Company ledger and dates.");
        const edition = Number(await ports.nextEdition(input)), now = ports.now();
        return ports.commitStatement({ actor, input, key, id, op, profile, sourceRevision, sourceFingerprint, snapshot, edition, now });
    }
    async function decideStatementPackage(actor: WorkContext, id: string, action: "issue" | "reject" | "cancel", raw: z.input<typeof reportingDecisionSchema>, key: string) {
        actor = ports.permission(actor, action === "cancel" ? "finance.write" : "finance.approve");
        companyScope(actor);
        const input = reportingDecisionSchema.parse(raw);
        await ports.ready(actor);
        const op = `finance.statement.${action}`, prior = await ports.replay<CommitStatementDecisionResult>(actor, key, op, id, input);
        if (prior)
            return prior;
        const row = await ports.package(actor, id);
        if (!row)
            throw ports.error(404, "STATEMENT_PACKAGE_NOT_FOUND", "Statement package not found.");
        if (row.status !== "draft" || Number(row.version) !== input.expectedVersion)
            throw ports.error(409, "VERSION_CONFLICT", "Refresh the prepared statement before reviewing it.");
        if (action !== "cancel" && samePerson(actor, row.prepared_by))
            throw ports.error(403, "INDEPENDENT_APPROVER_REQUIRED", "A different Finance approver must issue the statement package.");
        if (action === "cancel" && !samePerson(actor, row.prepared_by))
            ports.permission(actor, "finance.approve");
        if (action === "issue" && await ports.revision(String(row.chart_id)) !== Number(row.source_revision))
            throw ports.error(409, "REPORTING_SOURCE_CHANGED", "Ledger data changed after preparation. Retain this draft and prepare a new edition.");
        const snapshot = JSON.parse(String(row.snapshot_json)) as StatementSnapshot, html = action === "issue" ? statementHtml(snapshot, Number(row.edition)) : null, sha256 = html ? ports.hash(html) : null, now = ports.now(), status = action === "issue" ? "issued" : action === "reject" ? "rejected" : "cancelled", next = input.expectedVersion + 1;
        return ports.commitStatementDecision({ actor, id, action, input, key, op, row, html, sha256, now, status, next });
    }
    async function statementPackageExport(actor: WorkContext, id: string) { const detail = await statementPackageDetail(actor, id); if (detail.row.status !== "issued" || !detail.row.export_html || !detail.row.export_sha256)
        throw ports.error(409, "STATEMENT_NOT_ISSUED", "An independent Finance approver must issue this edition before export."); return { content: String(detail.row.export_html), sha256: String(detail.row.export_sha256), edition: Number(detail.row.edition) }; }
    async function prepareYearClosing(actor: WorkContext, raw: z.input<typeof yearClosingSchema>, key: string) {
        actor = ports.permission(actor, "finance.write");
        companyScope(actor);
        const input = yearClosingSchema.parse(raw);
        await ports.ready(actor);
        const id = `year-closing-${key}`, op = "finance.year-close.prepare", prior = await ports.replay<CommitYearClosingResult>(actor, key, op, id, input);
        if (prior)
            return prior;
        const { chart, profile } = await chartAndProfile(actor, input.chartId, input.startDate), dates = fiscalDates(input.startDate, profile.fiscal_start_month);
        if (dates.start !== input.startDate || dates.end !== input.endDate)
            throw ports.error(422, "FISCAL_DATES_INVALID", "Close one full fiscal year using the approved Company fiscal start month.");
        if (await ports.existingClosing(actor, input))
            throw ports.error(409, "FISCAL_CLOSING_EXISTS", "Review the current fiscal closing. A posted year retains its original transfer and subsequent adjustments use an open period.");
        const period = await ports.period(actor, input);
        if (!period || period.state !== "open" || period.start_date > input.endDate || period.end_date !== input.endDate)
            throw ports.error(409, "FISCAL_PERIOD_NOT_READY", "Use the open final accounting period ending on the fiscal year-end date.");
        const equity = await ports.equityAccount(actor, input, chart.base_currency);
        if (!equity)
            throw ports.error(422, "RETAINED_EARNINGS_ACCOUNT_REQUIRED", "Select an active retained earnings account in this ledger's functional currency.");
        const sourceRevision = await ports.revision(input.chartId), accounts = await ports.totals(actor, input.chartId, input.startDate, input.endDate), income = accounts.filter(account => ["revenue", "other_income", "expense", "other_expense"].includes(account.category));
        if (income.some(account => Number(account.opening_minor) !== 0))
            throw ports.error(409, "PREVIOUS_FISCAL_RESULT_OPEN", "Close or reconcile the preceding fiscal result before transferring the current year.");
        const evidence = await ports.evidence(actor, input.chartId, input.endDate), sourceFingerprint = digest({ chart, profile, accounts, evidence }), profit = amount(income.reduce((sum, account) => sum - Number(account.balance_minor), 0)), now = ports.now(), snapshot = { chart, profile, accounts: income, profitMinor: profit, sourceFingerprint };
        if (await ports.revision(input.chartId) !== sourceRevision)
            throw ports.error(409, "REPORTING_SOURCE_CHANGED", "Ledger data changed while preparing fiscal closing.");
        return ports.commitYearClosing({ actor, input, key, id, op, profile, sourceRevision, sourceFingerprint, snapshot, profit, now });
    }
    async function decideYearClosing(actor: WorkContext, id: string, action: "post" | "reject" | "cancel", raw: z.input<typeof reportingDecisionSchema>, key: string) {
        actor = ports.permission(actor, action === "cancel" ? "finance.write" : "finance.approve");
        companyScope(actor);
        const input = reportingDecisionSchema.parse(raw);
        await ports.ready(actor);
        const op = `finance.year-close.${action}`, prior = await ports.replay<CommitClosingDecisionResult>(actor, key, op, id, input);
        if (prior)
            return prior;
        const row = await ports.closing(actor, id);
        if (!row)
            throw ports.error(404, "YEAR_CLOSING_NOT_FOUND", "Fiscal closing not found.");
        if (row.status !== "draft" || Number(row.version) !== input.expectedVersion)
            throw ports.error(409, "VERSION_CONFLICT", "Refresh the fiscal closing before review.");
        if (action !== "cancel" && samePerson(actor, row.prepared_by))
            throw ports.error(403, "INDEPENDENT_APPROVER_REQUIRED", "A different Finance approver must post fiscal closing.");
        if (action === "cancel" && !samePerson(actor, row.prepared_by))
            ports.permission(actor, "finance.approve");
        if (action === "post" && await ports.revision(String(row.chart_id)) !== Number(row.source_revision))
            throw ports.error(409, "REPORTING_SOURCE_CHANGED", "Ledger data changed after fiscal closing preparation.");
        const snapshot = JSON.parse(String(row.snapshot_json)) as {
            chart: Chart;
            accounts: Account[];
            profitMinor: number;
        }, lines: JournalLineInput[] = snapshot.accounts.filter(account => Number(account.balance_minor) !== 0).map(account => ({ accountId: account.id, debit: account.balance_minor < 0 ? minorToDecimal(-Number(account.balance_minor)) : "0", credit: account.balance_minor > 0 ? minorToDecimal(Number(account.balance_minor)) : "0", description: "Transfer fiscal result to retained earnings" }));
        if (snapshot.profitMinor !== 0)
            lines.push({ accountId: String(row.retained_earnings_account_id), debit: snapshot.profitMinor < 0 ? minorToDecimal(-snapshot.profitMinor) : "0", credit: snapshot.profitMinor > 0 ? minorToDecimal(snapshot.profitMinor) : "0" });
        const journal = action === "post" && lines.length ? await ports.prepareJournal(actor, { chartId: String(row.chart_id), periodId: String(row.period_id), entryDate: String(row.end_date), currency: snapshot.chart.base_currency, memo: `Fiscal closing ${row.start_date} to ${row.end_date}`, sourceType: "fiscal_close", sourceId: id, isOpening: false, lines }, key) : null;
        if (journal)
            await ports.prepareMoney(actor, { sourceType: "finance_year_closing", sourceId: id, amountKind: "profit_transfer", transactionDate: String(row.end_date), currency: snapshot.chart.base_currency, currencyScale: 2, amountUnits: String(snapshot.profitMinor) });
        const now = ports.now(), status = action === "post" ? "posted" : action === "reject" ? "rejected" : "cancelled", next = input.expectedVersion + 1;
        return ports.commitClosingDecision({ actor, id, action, input, key, op, row, now, status, next, journalId: journal?.result.id ?? null });
    }
    async function statementSourceAccounts(actor: WorkContext, id: string, args: Record<string, string>, page = 1) {
        const { row, snapshot } = await statementPackageDetail(actor, id);
        const [current, evidence] = await Promise.all([ports.totals(actor, String(row.chart_id), snapshot.current.dates.start, snapshot.current.dates.end), ports.evidence(actor, String(row.chart_id), snapshot.current.dates.end)]);
        if (digest({ chart: snapshot.chart, profile: snapshot.profile, accounts: current, evidence }) !== snapshot.sourceFingerprint)
            throw ports.error(409, 'STATEMENT_SOURCES_CHANGED', 'The ledger has changed since this retained edition. Open a current report or the retained statement; current entries cannot reproduce this edition.');
        const period = z.enum(['current', 'comparative']).default('current').parse(args.period), set = snapshot[period];
        const accounts = period === 'current' ? current : await ports.totals(actor, String(row.chart_id), set.dates.start, set.dates.end);
        const presentation = JSON.parse(String(row.presentation_json)) as Presentation, byAccount = new Map(presentation.map(item => [item.accountId, item.line]));
        const incomeCategories = new Set(['revenue', 'other_income', 'expense', 'other_expense']);
        const selection = args.line ?? 'equity';
        const income = selection === 'result' || (selection in financialPresentation && financialPresentation[selection as FinancialPresentationCode].statement === 'income');
        const matching = accounts.filter(account => args.accountId ? account.id === args.accountId : selection === 'equity' ? account.category === 'equity' || incomeCategories.has(account.category) : selection === 'unclosed' || selection === 'result' ? incomeCategories.has(account.category) : byAccount.get(account.id) === selection || selection === 'retained_earnings' && incomeCategories.has(account.category));
        const value = (account: Account) => Number(income ? account.income_minor : account.balance_minor) * (['asset', 'expense', 'other_expense'].includes(account.category) && selection !== 'result' ? 1 : -1);
        const sourceArgs = { chartId: String(row.chart_id), endDate: set.dates.end, ...(income ? { startDate: set.dates.start, excludeYearClose: '1' } : {}), statementId: id };
        return { row: { name: `${selection} · ${set.dates.start} – ${set.dates.end}`, currency: snapshot.chart.base_currency, total_minor: matching.reduce((sum, account) => sum + value(account), 0), version: row.edition }, rows: matching.slice((page - 1) * 30, page * 30).map(account => ({ ...account, balance: minorToDecimal(value(account)) })), sourceArgs, page, hasNext: matching.length > page * 30 };
    }
    return { reportingWorkspace, statementPackageDetail, prepareStatementPackage, decideStatementPackage, statementPackageExport, prepareYearClosing, decideYearClosing, statementSourceAccounts };
}


export type CommitStatementResult = { id: string; status: string; version: number; edition: number; sourceFingerprint: string; };
export type CommitStatementDecisionResult = { id: string; status: string; version: number; edition: number; sha256: string | null; };
export type CommitYearClosingResult = { id: string; status: string; version: number; profitMinor: number; sourceFingerprint: string; };
export type CommitClosingDecisionResult = { id: string; status: string; version: number; profitMinor: number; journalId: string | null; };
