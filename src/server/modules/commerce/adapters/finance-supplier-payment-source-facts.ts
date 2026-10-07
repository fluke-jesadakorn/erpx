/** Fixed aliases retain the exact original configured cash account and pending-return predicates. */
export function financeSupplierPaymentSourceFacts(){return {
 configuredCashAccount:"a.id=(SELECT cash_account_id FROM commerce_settings WHERE company_id=a.company_id)",
 noPendingReturn:"NOT EXISTS(SELECT 1 FROM commerce_returns r WHERE r.invoice_id=i.id AND r.status IN ('requested','approved'))",
 pendingReturn:"EXISTS(SELECT 1 FROM commerce_returns cr WHERE cr.invoice_id=i.id AND cr.status IN ('requested','approved'))",
} as const;}
