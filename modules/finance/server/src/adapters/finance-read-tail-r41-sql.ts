/** G4 / retire.finance.read-tail-r41 safe NEW read-tail SQL families.
 *
 * Every export renders byte-identical text to the literal it replaces. Callers
 * keep their own binds, result types and transaction; nothing here is cached or
 * executed. Row-lock text, tax/native guards and r15-r40 helpers are unchanged.
 */
import { financePaymentReversalSources } from './supplier-payment-correction-sources';

export const APPROVED_SECONDARY_CHART_CORRELATED_GUARD = "NOT EXISTS(SELECT 1 FROM finance_book_rules r WHERE r.company_id=a.company_id AND r.secondary_chart_id=a.chart_id AND r.status='approved')";

export const withholdingRemittedGuardSql = (assessmentRef: string) => `SELECT 1 FROM finance_withholding_remittance_items i JOIN finance_withholding_remittances r ON r.id=i.remittance_id WHERE i.assessment_id=${assessmentRef} AND r.status='confirmed'`;

export const ORIGINAL_PAYMENT_REVERSAL_GUARD = `AND (original.status='settled' OR EXISTS(SELECT 1 FROM (${financePaymentReversalSources}) reversal WHERE reversal.id=original.id AND reversal.company_id=original.company_id))`;
