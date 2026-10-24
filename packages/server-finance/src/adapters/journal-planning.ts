

import type { JournalPlan } from '@erpx/server-finance/application/journal';
import type { JournalInput } from '@erpx/server-finance/domain/finance';

import { createJournalPreparation } from '@erpx/server-finance/application/journal';
import { createEnsurePeriod } from '@erpx/server-finance/application/period-policy';
import { createFunctionalFx } from '@erpx/server-finance/application/functional-fx';

import { createJournalSchema, type FinanceRow } from '@erpx/server-finance/domain/finance';

import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import { z } from "zod";
import {assertion} from "@erpx/server-workflow/public/command-assertions";

import type { WorkContext } from "@erpx/server-identity_access/public/authorization";

import { ApiError } from "@erpx/server-runtime/public/http";

import {companyRates} from '@erpx/server-finance/public/company-money-rates';

import { requireFinanceMoneyScale } from "@erpx/server-finance/public/finance-money-policy";

export async function confirmedFunctionalFx(write: SqlDatabase, actor: WorkContext, currency: string, baseCurrency: string, date: string) { return createFunctionalFx({ rates: (companyId) => companyRates(write, companyId), error: (...args) => new ApiError(...args) })(actor, currency, baseCurrency, date); }
export async function ensurePeriod(write: SqlDatabase, actor: WorkContext, periodId: string, entryDate: string, allowClosing = false) { return createEnsurePeriod({ error: (...args) => new ApiError(...args), period: (companyId, periodId) => write.prepare('SELECT * FROM finance_periods WHERE id=? AND company_id=?').bind(periodId, companyId).first<FinanceRow>(), coveredDate: async (companyId, date) => !!await write.prepare("SELECT 1 FROM finance_periods WHERE company_id=? AND state IN ('closed','closing') AND ? BETWEEN start_date AND end_date LIMIT 1").bind(companyId, date).first() })(actor, periodId, entryDate, allowClosing); }
export async function prepareJournal(write: SqlDatabase, actor: WorkContext, input: z.infer<typeof createJournalSchema>, key: string, status: "draft" | "posted" = "draft") { const plan = await journalPreparation(write)(actor, input, key, status); return journalStatements(write, actor, input, key, status, plan); }
export function journalPreparation(write: SqlDatabase) {
    return createJournalPreparation({ error: (...args) => new ApiError(...args), now: Date.now, chart: (companyId, chartId) => write.prepare("SELECT id,base_currency FROM finance_charts WHERE id=? AND company_id=? AND status='active'").bind(chartId, companyId).first<{
            id: string;
            base_currency: string;
        }>(), period: (actor, periodId, date) => ensurePeriod(write, actor, periodId, date), moneyScale: (companyId, currency) => requireFinanceMoneyScale(write, companyId, currency), functionalFx: (actor, currency, baseCurrency, date) => confirmedFunctionalFx(write, actor, currency, baseCurrency, date), accounts: async (companyId, chartId, accountIds) => (await write.prepare(`SELECT id FROM finance_accounts WHERE company_id=? AND chart_id=? AND status='active' AND id IN (${accountIds.map(() => "?").join(",")})`).bind(companyId, chartId, ...accountIds).all<{
            id: string;
        }>()).results, openingExists: async (companyId, chartId) => !!await write.prepare("SELECT id FROM finance_journals WHERE company_id=? AND chart_id=? AND is_opening=1 AND status IN ('draft','in_review','posted') LIMIT 1").bind(companyId, chartId).first() });
}
export function journalStatements(write: SqlDatabase, actor: WorkContext, input: JournalInput, key: string, status: "draft" | "posted", plan: JournalPlan) {
    const { fx, accountIds, journalId, number, now, values, preparedFxEvidence } = plan;
    const guards: SqlStatement[] = [
        assertion(write, key, "posting-period", "EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND start_date<=? AND end_date>=?)", [input.periodId, actor.companyId, input.entryDate, input.entryDate]),
        assertion(write, key, "posting-chart", "EXISTS(SELECT 1 FROM finance_charts WHERE id=? AND company_id=? AND status='active')", [input.chartId, actor.companyId]),
        ...(input.isOpening ? [assertion(write, key, "opening-balance", "NOT EXISTS(SELECT 1 FROM finance_journals WHERE company_id=? AND chart_id=? AND is_opening=1 AND status IN ('draft','in_review','posted'))", [actor.companyId, input.chartId])] : []),
        ...accountIds.map((accountId, index) => assertion(write, key, `posting-account-${index}`, "EXISTS(SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=? AND status='active')", [accountId, actor.companyId, input.chartId])),
    ];
    const steps: SqlStatement[] = [...guards,
        write.prepare("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,is_opening,status,created_by,posted_by,posted_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(journalId, actor.companyId, input.chartId, input.periodId, number, input.entryDate, input.currency, fx, input.sourceType, input.sourceId ?? null, input.memo, input.isOpening ? 1 : 0, status, actor.userId, status === "posted" ? actor.userId : null, status === "posted" ? now : null, now, now),
        ...values.map((l, index) => write.prepare("INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(`${journalId}-${index}`, journalId, l.accountId, l.description ?? "", l.debit, l.credit, l.baseDebit, l.baseCredit, JSON.stringify(l.dimensions ?? {}), index)),
    ];
    // Owner adapters consume these exact retained amounts; public journal results
    // and command receipts keep their existing shape.
    return { steps, guards, preparedLines: values, preparedFxEvidence, result: plan.result };
}
