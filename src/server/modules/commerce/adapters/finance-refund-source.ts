import { commercialOwnerProjectionSources as financeProjectionSources } from "../../finance/public/commercial-owner-projection-sources";
import 'server-only';
import type { SqlDatabase } from '@/server/modules/runtime/public/runtime/types';
import type { WorkContext } from '@/server/modules/identity_access/public/authorization';
import { assertion } from '@/server/modules/workflow/public/commands';
type Row = Record<string, string | number | null>;
export async function refundCashAccounts(db: SqlDatabase, actor: WorkContext, owner: Row) { return (await db.prepare(`SELECT id,code,name FROM (${financeProjectionSources.finance_accounts}) finance_accounts WHERE company_id=? AND chart_id=? AND status='active' AND category='asset' AND id<>? AND (id=(SELECT cash_account_id FROM commerce_settings WHERE company_id=finance_accounts.company_id) OR (?::text IS NOT NULL AND control_kind='bank')) ORDER BY code`).bind(actor.companyId, owner.chart_id, owner.balance_account_id, owner.billing_correction_id ?? null).all<Row>()).results; }
export function refundCashAccountGuard(db: SqlDatabase, actor: WorkContext, key: string, facts: {
    cashAccountId: string;
    chartId: Row[string];
    balanceAccountId: Row[string];
    billingCorrectionId: Row[string];
}) { return assertion(db, key, 'cash-account', `EXISTS(SELECT 1 FROM (${financeProjectionSources.finance_accounts}) finance_accounts WHERE id=? AND company_id=? AND chart_id=? AND category='asset' AND status='active' AND id<>? AND (id=(SELECT cash_account_id FROM commerce_settings WHERE company_id=finance_accounts.company_id) OR (?::text IS NOT NULL AND control_kind='bank')))`, [facts.cashAccountId, actor.companyId, facts.chartId, facts.balanceAccountId, facts.billingCorrectionId]); }
export function refundForeignCreditGuard(db: SqlDatabase, key: string, id: Row[string], sourceBase: number) { return assertion(db, key, 'source-credit', `EXISTS(SELECT 1 FROM (${financeProjectionSources.finance_refund_obligations}) o JOIN commerce_returns r ON r.id=o.return_id JOIN (${financeProjectionSources.finance_journals}) j ON j.id=o.source_credit_journal_id AND j.id=r.credit_journal_id AND j.company_id=o.company_id AND j.chart_id=o.chart_id WHERE o.id=? AND o.base_outstanding_minor=? AND j.status IN ('posted','reversed'))`, [id, sourceBase]); }
export async function billingCorrectionExternalCredit(db: SqlDatabase, invoiceId: string) { return db.prepare("SELECT 1 FROM commerce_returns WHERE invoice_id=? AND status IN ('requested','approved','posted')").bind(invoiceId).first(); }
