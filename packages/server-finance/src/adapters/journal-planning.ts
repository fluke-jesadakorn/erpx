import { OPEN_PERIOD_GUARD_SQL, OPEN_PERIOD_IDS_SQL } from './finance-read-tail-r16-sql';
import { ACCOUNT_SELECT_PREFIX } from './finance-read-tail-r30-sql';
import { ACTIVE_CHART_BASE_CURRENCY_BY_ID_SQL } from './finance-read-tail-r32-sql';


import type { JournalPlan } from '@erpx/server-finance/application/journal';
import { financeJournalPlanning } from '@erpx/server-collaboration/adapters/native-finance-journal-planning';
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
    return createJournalPreparation({ error: (...args) => new ApiError(...args), now: Date.now, chart: (companyId, chartId) => write.prepare(ACTIVE_CHART_BASE_CURRENCY_BY_ID_SQL).bind(chartId, companyId).first<{
            id: string;
            base_currency: string;
        }>(), period: (actor, periodId, date) => ensurePeriod(write, actor, periodId, date), moneyScale: (companyId, currency) => requireFinanceMoneyScale(write, companyId, currency), functionalFx: (actor, currency, baseCurrency, date) => confirmedFunctionalFx(write, actor, currency, baseCurrency, date), accounts: async (companyId, chartId, accountIds) => (await write.prepare(`${ACCOUNT_SELECT_PREFIX}company_id=? AND chart_id=? AND status='active' AND id IN (${accountIds.map(() => "?").join(",")})`).bind(companyId, chartId, ...accountIds).all<{
            id: string;
        }>()).results, openingExists: async (companyId, chartId) => !!await write.prepare("SELECT id FROM finance_journals WHERE company_id=? AND chart_id=? AND is_opening=1 AND status IN ('draft','in_review','posted') LIMIT 1").bind(companyId, chartId).first() });
}
export async function journalStatements(write: SqlDatabase, actor: WorkContext, input: JournalInput, key: string, status: "draft" | "posted", plan: JournalPlan) {
    const { fx, accountIds, journalId, number, now, values, preparedFxEvidence } = plan;
    const guards: SqlStatement[] = [
        assertion(write, key, "posting-period", OPEN_PERIOD_GUARD_SQL, [input.periodId, actor.companyId, input.entryDate, input.entryDate]),
        assertion(write, key, "posting-chart", "EXISTS(SELECT 1 FROM finance_charts WHERE id=? AND company_id=? AND status='active')", [input.chartId, actor.companyId]),
        ...(input.isOpening ? [assertion(write, key, "opening-balance", "NOT EXISTS(SELECT 1 FROM finance_journals WHERE company_id=? AND chart_id=? AND is_opening=1 AND status IN ('draft','in_review','posted'))", [actor.companyId, input.chartId])] : []),
        ...accountIds.map((accountId, index) => assertion(write, key, `posting-account-${index}`, "EXISTS(SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=? AND status='active')", [accountId, actor.companyId, input.chartId])),
    ];
    const steps: SqlStatement[] = [...guards,
        await financeJournalPlanning(write, actor.companyId, actor.userId).journal({ journalId, companyId: actor.companyId, chartId: input.chartId, periodId: input.periodId, number, entryDate: input.entryDate, currency: input.currency, fx, sourceType: input.sourceType, sourceId: input.sourceId ?? null, memo: input.memo, isOpening: input.isOpening, status, now, userId: actor.userId }),
        ...await Promise.all(values.map(async (l, index) => await financeJournalPlanning(write, actor.companyId, actor.userId).line({ journalId, id: `${journalId}-${index}`, accountId: l.accountId, description: l.description ?? "", debit: l.debit, credit: l.credit, baseDebit: l.baseDebit, baseCredit: l.baseCredit, dimensions: l.dimensions ?? {}, ordinal: index }))),
    ];
    // Owner adapters consume these exact retained amounts; public journal results
    // and command receipts keep their existing shape.
    return { steps, guards, preparedLines: values, preparedFxEvidence, result: plan.result };
}
