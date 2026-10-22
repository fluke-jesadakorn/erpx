import { z } from 'zod';
import { financialNoteTitles, financialPresentation, type FinancialPresentationCode } from "@erpx/feature-finance/lib/finance-reporting";
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import type { FinanceError } from '@erpx/server-finance/domain/invoice-line-revision';
const date = z.iso.date(), reason = z.string().trim().min(10).max(4000), version = z.number().int().positive();
const codes = Object.keys(financialPresentation) as [
    FinancialPresentationCode,
    ...FinancialPresentationCode[]
];
export const statementPackageSchema = z.object({ chartId: z.string().min(1), startDate: date, endDate: date, comparativeStartDate: date, comparativeEndDate: date, previousPackageId: z.string().min(1).optional(),
    presentation: z.array(z.object({ accountId: z.string().min(1), line: z.enum(codes) })).min(1).max(1000),
    notes: z.object({ basis: reason, policies: reason, going_concern: reason, judgments: reason, disclosures: reason }), reason }).strict();
export const reportingDecisionSchema = z.object({ expectedVersion: version, reason }).strict();
export const yearClosingSchema = z.object({ chartId: z.string().min(1), startDate: date, endDate: date, periodId: z.string().min(1), retainedEarningsAccountId: z.string().min(1), reason }).strict();
export type Profile = {
    id: string;
    version: number;
    statutory_chart_id: string;
    fiscal_start_month: number;
    effective_date: string;
    policy_evidence: string;
    eligibility_evidence: string;
    inventory_policy: string;
    depreciation_policy: string;
    tax_basis: string;
};
export type Chart = {
    id: string;
    name: string;
    base_currency: string;
    status: string;
    company_name: string;
};
export type Account = {
    id: string;
    code: string;
    name: string;
    category: string;
    currency: string;
    status: string;
    balance_minor: number;
    opening_minor: number;
    income_minor: number;
    owner_movements_minor: number;
    closing_transfer_minor: number;
};
export type Package = Record<string, string | number | null>;
export type Presentation = z.infer<typeof statementPackageSchema>["presentation"];
export type StatementSet = {
    dates: {
        start: string;
        end: string;
    };
    position: Array<{
        code: FinancialPresentationCode;
        label: string;
        category: string;
        amountMinor: number;
    }>;
    income: Array<{
        code: FinancialPresentationCode;
        label: string;
        category: string;
        amountMinor: number;
    }>;
    assetsMinor: number;
    liabilitiesMinor: number;
    equityMinor: number;
    profitMinor: number;
    equity: {
        openingMinor: number;
        ownerMovementsMinor: number;
        profitMinor: number;
        closingMinor: number;
        components: Array<{
            accountId: string;
            code: string;
            name: string;
            openingMinor: number;
            ownerMovementsMinor: number;
            transferMinor: number;
            closingMinor: number;
        }>;
        unclosed: {
            openingMinor: number;
            profitMinor: number;
            transferMinor: number;
            closingMinor: number;
        };
    };
};
export type StatementSnapshot = {
    standard: string;
    certified: false;
    company: string;
    chart: Chart;
    profile: Profile;
    current: StatementSet;
    comparative: StatementSet;
    notes: z.infer<typeof statementPackageSchema>["notes"];
    sourceFingerprint: string;
    sourceRevision: number;
    standardsSource: {
        url: string;
        effectiveFrom: string;
        verifiedOn: string;
    };
};
const escape = (value: unknown) => String(value ?? "").replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;").replaceAll('"', "&quot;").replaceAll("'", "&#39;");
export function statementHtml(snapshot: StatementSnapshot, edition: number) {
    const current = snapshot.current, comparison = snapshot.comparative, number = (value: number) => minorToDecimal(value), table = (kind: "position" | "income") => {
        const a = new Map(current[kind].map(row => [row.code, row])), b = new Map(comparison[kind].map(row => [row.code, row]));
        const rows = codes.filter(code => a.has(code) || b.has(code)).map(code => `<tr><td>${escape(financialPresentation[code].label)}</td><td>${number(a.get(code)?.amountMinor ?? 0)}</td><td>${number(b.get(code)?.amountMinor ?? 0)}</td></tr>`).join("");
        return `<table><thead><tr><th>Account presentation</th><th>${escape(current.dates.end)}</th><th>${escape(comparison.dates.end)}</th></tr></thead><tbody>${rows}</tbody></table>`;
    };
    const equity = (set: StatementSet) => `<tr><td>${escape(set.dates.start)} to ${escape(set.dates.end)}</td><td>${number(set.equity.openingMinor)}</td><td>${number(set.equity.ownerMovementsMinor)}</td><td>${number(set.profitMinor)}</td><td>${number(set.equityMinor)}</td></tr>`;
    return `<!doctype html><html lang="en"><meta charset="utf-8"><title>${escape(snapshot.company)} · Financial statements edition ${edition}</title><style>body{font:15px/1.6 system-ui,sans-serif;color:#17202a;max-width:1000px;margin:48px auto;padding:0 24px}h1,h2,h3{line-height:1.2}table{width:100%;border-collapse:collapse;margin:20px 0}th,td{padding:10px;border-bottom:1px solid #cbd5e1;text-align:right}th:first-child,td:first-child{text-align:left}small{color:#475569}p{white-space:pre-wrap}section{break-inside:avoid}a{color:inherit}@media print{body{margin:0;font-size:11pt}h2{break-after:avoid}}</style><body><h1>${escape(snapshot.company)}</h1><h2>Financial statements · Edition ${edition}</h2><p>${escape(current.dates.start)} to ${escape(current.dates.end)} · Comparative ${escape(comparison.dates.start)} to ${escape(comparison.dates.end)}<br>Ledger: ${escape(snapshot.chart.name)} · Functional currency: ${escape(snapshot.chart.base_currency)}</p><small>Prepared under the Company's approved ${escape(snapshot.standard)} policies. Independent internal approval; certification: not claimed.</small><section><h2>Statement of financial position</h2>${table("position")}<p>Assets ${number(current.assetsMinor)} · Liabilities ${number(current.liabilitiesMinor)} · Equity ${number(current.equityMinor)}</p></section><section><h2>Income statement</h2>${table("income")}<p>Current profit or loss ${number(current.profitMinor)} · Comparative ${number(comparison.profitMinor)}</p></section><section><h2>Statement of changes in equity</h2><table><thead><tr><th>Reporting period</th><th>Opening equity</th><th>Owner movements</th><th>Profit or loss</th><th>Closing equity</th></tr></thead><tbody>${equity(current)}${equity(comparison)}</tbody></table>${[current, comparison].map(set => `<h3>${escape(set.dates.end)} · Equity components</h3><table><thead><tr><th>Component</th><th>Opening</th><th>Owner movements</th><th>Closing transfers</th><th>Closing</th></tr></thead><tbody>${set.equity.components.map(row => `<tr><td>${escape(row.code)} · ${escape(row.name)}</td><td>${number(row.openingMinor)}</td><td>${number(row.ownerMovementsMinor)}</td><td>${number(row.transferMinor)}</td><td>${number(row.closingMinor)}</td></tr>`).join("")}<tr><td>Unclosed result</td><td>${number(set.equity.unclosed.openingMinor)}</td><td>${number(set.equity.unclosed.profitMinor)}</td><td>${number(set.equity.unclosed.transferMinor)}</td><td>${number(set.equity.unclosed.closingMinor)}</td></tr></tbody></table>`).join("")}</section><section><h2>Notes to the financial statements</h2>${Object.entries(financialNoteTitles).map(([key, title], index) => `<h3>${index + 1}. ${escape(title)}</h3><p>${escape(snapshot.notes[key as keyof typeof snapshot.notes])}</p>`).join("")}</section><small>Source fingerprint: ${escape(snapshot.sourceFingerprint)}<br>Approved policy revision: ${escape(snapshot.profile.id)} / ${snapshot.profile.version}<br>Presentation basis: <a href="${escape(snapshot.standardsSource.url)}">TFAC NPAEs</a> · effective ${snapshot.standardsSource.effectiveFrom}</small></body></html>`;
}
export function fiscalDates(start: string, fiscalStartMonth: number) { const year = Number(start.slice(0, 4)), expectedStart = `${year}-${String(fiscalStartMonth).padStart(2, "0")}-01`, end = new Date(Date.UTC(year + 1, fiscalStartMonth - 1, 0)).toISOString().slice(0, 10); return { start: expectedStart, end }; }
export function createReportingPolicy(error: FinanceError) {
    function amount(value: number) { if (!Number.isSafeInteger(value))
        throw error(422, "MONEY_OVERFLOW", "A financial statement exceeds supported integer precision."); return value; }
    function statement(accounts: Account[], presentation: Presentation, start: string, end: string): StatementSet {
        const byAccount = new Map(presentation.map(row => [row.accountId, row.line])), incomeCategories = new Set(["revenue", "other_income", "expense", "other_expense"]);
        const balances = new Map<FinancialPresentationCode, number>(), income = new Map<FinancialPresentationCode, number>();
        const equityComponents: StatementSet["equity"]["components"] = [], profit = accounts.filter(account => incomeCategories.has(account.category)).reduce((sum, account) => sum - Number(account.income_minor), 0);
        const unclosed = accounts.filter(account => incomeCategories.has(account.category)).reduce((sum, account) => sum - Number(account.balance_minor), 0), openingUnclosed = accounts.filter(account => incomeCategories.has(account.category)).reduce((sum, account) => sum - Number(account.opening_minor), 0);
        for (const account of accounts) {
            const code = byAccount.get(account.id), nonzero = [account.balance_minor, account.opening_minor, account.income_minor].some(value => Number(value) !== 0);
            if (nonzero && !code)
                throw error(422, "STATEMENT_PRESENTATION_REQUIRED", `Classify ${account.code} · ${account.name} for the current and comparative statements.`);
            if (!code)
                continue;
            const definition = financialPresentation[code];
            if (definition.category !== account.category)
                throw error(422, "STATEMENT_PRESENTATION_INVALID", `The reporting line for ${account.code} does not match its immutable account category.`);
            const positive = ["asset", "expense", "other_expense"].includes(account.category) ? 1 : -1;
            if (definition.statement === "position")
                balances.set(code, amount((balances.get(code) ?? 0) + Number(account.balance_minor) * positive));
            else
                income.set(code, amount((income.get(code) ?? 0) + Number(account.income_minor) * positive));
            if (account.category === "equity")
                equityComponents.push({ accountId: account.id, code: account.code, name: account.name, openingMinor: -Number(account.opening_minor), ownerMovementsMinor: Number(account.owner_movements_minor), transferMinor: Number(account.closing_transfer_minor), closingMinor: -Number(account.balance_minor) });
        }
        balances.set("retained_earnings", amount((balances.get("retained_earnings") ?? 0) + unclosed));
        const position = [...balances].map(([code, amountMinor]) => ({ code, label: financialPresentation[code].label, category: financialPresentation[code].category, amountMinor }));
        const incomeRows = [...income].map(([code, amountMinor]) => ({ code, label: financialPresentation[code].label, category: financialPresentation[code].category, amountMinor }));
        const category = (name: string) => position.filter(row => row.category === name).reduce((sum, row) => sum + row.amountMinor, 0), assets = amount(category("asset")), liabilities = amount(category("liability")), equity = amount(category("equity"));
        if (assets !== liabilities + equity)
            throw error(409, "STATEMENT_LEDGER_UNBALANCED", "Financial position does not reconcile to the posted ledger.");
        const openingEquity = amount(equityComponents.reduce((sum, row) => sum + row.openingMinor, openingUnclosed)), ownerMovements = amount(equityComponents.reduce((sum, row) => sum + row.ownerMovementsMinor, 0)), transfer = amount(equityComponents.reduce((sum, row) => sum + row.transferMinor, 0));
        if (openingEquity + ownerMovements + profit !== equity || openingUnclosed + profit - transfer !== unclosed)
            throw error(409, "STATEMENT_EQUITY_UNRECONCILED", "Owner movements and current result do not reconcile to equity.");
        return { dates: { start, end }, position, income: incomeRows, assetsMinor: assets, liabilitiesMinor: liabilities, equityMinor: equity, profitMinor: amount(profit), equity: { openingMinor: openingEquity, ownerMovementsMinor: ownerMovements, profitMinor: amount(profit), closingMinor: equity, components: equityComponents, unclosed: { openingMinor: openingUnclosed, profitMinor: amount(profit), transferMinor: -transfer, closingMinor: unclosed } } };
    }
    return { amount, statement };
}
export type StatementInput = z.infer<typeof statementPackageSchema>;
export type ReportingDecision = z.infer<typeof reportingDecisionSchema>;
export type YearClosingInput = z.infer<typeof yearClosingSchema>;
export type ReportingRow = Record<string, unknown>;
