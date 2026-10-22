/** The native claim guard stays inside the supplied Finance transaction's assertion statement. */
export function financeContractorPaymentClaimFacts(){return {
 noActiveContractorClaim:"NOT EXISTS(SELECT 1 FROM hr_payment_batches h WHERE h.company_id=i.company_id AND h.source_type='contractor' AND h.source_id=i.id AND h.status<>'cancelled')",
} as const;}
