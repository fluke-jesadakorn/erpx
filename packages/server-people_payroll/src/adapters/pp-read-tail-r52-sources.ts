import 'server-only';
import {expensePaymentCancellations,expensePaymentTreasurySql} from './expense-payment-cancellation-sources';

/** Canonical parameterized read builders for the native expense payment cancellation
 * closure (G4 read-tail r52). Each builder owns one read-only literal copied verbatim
 * from its former inline copies; consumers import it and keep no local copy. The
 * `terminal` / `full` switches select the exact former tail bytes, including the
 * Treasury and authority tails and the spread authority binds stay at the caller.
 * Lock-bearing reads, native kinds, cross-owner projections and r1-r51 modules stay
 * untouched. */

/** Cancellation history image. `terminal` reproduces the EXISTS terminal + Treasury
 * tail form; otherwise the plain bound/draft-or-bound image is reproduced. */
export const ppRd52PaymentImage = (terminal: boolean) =>
  `SELECT to_jsonb(d)::text draft,to_jsonb(e)::text expense FROM expense_payment_drafts d JOIN finance_expenses e ON e.id=d.expense_id AND e.company_id=d.company_id${terminal ? `
    WHERE d.id=? AND d.expense_id=? AND d.company_id=? AND EXISTS(SELECT 1 FROM (${expensePaymentCancellations}) terminal WHERE terminal.id=d.id AND terminal.company_id=d.company_id)
    AND (${expensePaymentTreasurySql('e')})` : ' WHERE d.id=? AND d.expense_id=? AND d.company_id=?'}`;

/** Cancellation validity probe. `full` reproduces the native-source + independent
 * tail; otherwise the Treasury tail sits inside the EXISTS. The authority fragment
 * and every spread bind stay owned by the caller. */
export const ppRd52PaymentValidity = (full: boolean, nativeSource: string, independent: string, authoritySql: string) =>
  `SELECT (EXISTS(SELECT 1 FROM expense_payment_drafts d JOIN finance_expenses e ON e.id=d.expense_id AND e.company_id=d.company_id WHERE d.id=? AND d.company_id=? AND to_jsonb(d)=?::jsonb AND to_jsonb(e)=?::jsonb${full ? '' : ` AND (${expensePaymentTreasurySql('e')})`})${full ? `
     AND (${nativeSource}) AND (${independent}) ` : ' '}AND (${authoritySql})) ok`;
