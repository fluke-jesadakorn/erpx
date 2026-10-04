/**
 * Reconstruct the subledger from dated accounting evidence. Mutable invoice and
 * payment statuses are workflow state, never a historical balance authority.
 * The two parameters are the Company's ID and the accounting cutoff date.
 */
export const invoiceHistoryCte = `WITH history_cutoff AS (
  SELECT ?::text company_id, ?::text as_of
), historical_invoice_balances AS (
  SELECT i.*, a.chart_id,
    COALESCE(j.entry_date,i.invoice_date) recognition_date,
    i.total_minor
      - COALESCE(settlements.transaction_minor,0)
      - COALESCE(credits.transaction_minor,0)
      - COALESCE(opening_reversal.transaction_minor,0)
      + COALESCE(refunds.transaction_minor,0) historical_outstanding_minor,
    (CASE WHEN i.base_total_minor=0 AND i.fx_rate_micros=1000000 THEN i.total_minor ELSE i.base_total_minor END)
      - COALESCE(settlements.base_minor,0)
      - COALESCE(credits.base_minor,0)
      - COALESCE(opening_reversal.base_minor,0)
      + COALESCE(refunds.base_minor,0)
      + COALESCE(revaluation.base_minor,0) historical_base_outstanding_minor,
    (j.id IS NULL OR COALESCE(settlements.legacy_payment_evidence,false)) legacy_date_evidence
  FROM finance_invoices i
  JOIN history_cutoff h ON h.company_id=i.company_id
  JOIN finance_accounts a ON a.id=CASE WHEN i.kind='receivable' THEN i.debit_account_id ELSE i.credit_account_id END AND a.company_id=i.company_id
  LEFT JOIN finance_journals j ON j.id=i.journal_id AND j.company_id=i.company_id
  LEFT JOIN LATERAL (
    SELECT SUM(x.amount_minor) transaction_minor,BOOL_OR(pj.id IS NULL) legacy_payment_evidence,
      SUM(CASE WHEN x.base_invoice_minor=0 AND i.fx_rate_micros=1000000 THEN x.amount_minor ELSE x.base_invoice_minor END) base_minor
    FROM finance_allocations x
    JOIN finance_payments p ON p.id=x.payment_id AND p.company_id=i.company_id
    LEFT JOIN finance_journals pj ON pj.id=p.journal_id AND pj.company_id=i.company_id
    LEFT JOIN finance_payment_reversals pr ON pr.payment_id=p.id
    LEFT JOIN finance_journals rj ON rj.id=pr.reversal_journal_id AND rj.company_id=i.company_id
    WHERE x.invoice_id=i.id AND COALESCE(pj.entry_date,p.payment_date)<=h.as_of
      AND (pj.status IN ('posted','reversed') OR (pj.id IS NULL AND p.status='settled'))
      AND (rj.id IS NULL OR rj.entry_date>h.as_of)
  ) settlements ON true
  LEFT JOIN LATERAL (
    SELECT SUM(r.total_minor) transaction_minor,
      SUM((SELECT COALESCE(SUM(CASE WHEN i.kind='receivable' THEN l.base_credit_minor-l.base_debit_minor ELSE l.base_debit_minor-l.base_credit_minor END),0)
        FROM finance_journal_lines l WHERE (l.journal_id=cj.id OR l.journal_id IN (
          SELECT fj.id FROM finance_journals fj WHERE fj.id=r.fx_journal_id AND fj.company_id=i.company_id
            AND fj.status IN ('posted','reversed') AND fj.entry_date<=h.as_of)) AND l.account_id=a.id)) base_minor
    FROM commerce_returns r
    JOIN finance_journals cj ON cj.id=r.credit_journal_id AND cj.company_id=i.company_id
    WHERE r.invoice_id=i.id AND cj.status IN ('posted','reversed') AND cj.entry_date<=h.as_of
  ) credits ON true
  LEFT JOIN LATERAL (
    SELECT SUM(CASE WHEN rj.id IS NOT NULL THEN i.total_minor ELSE 0 END) transaction_minor,
      SUM(CASE WHEN rj.id IS NULL THEN 0 WHEN i.base_total_minor=0 AND i.fx_rate_micros=1000000 THEN i.total_minor ELSE i.base_total_minor END) base_minor
    FROM finance_journals rj WHERE rj.company_id=i.company_id AND rj.source_type='reversal'
      AND rj.source_id=j.id AND rj.status IN ('posted','reversed') AND rj.entry_date<=h.as_of
      AND i.source_type='opening_import'
  ) opening_reversal ON true
  LEFT JOIN LATERAL (
    SELECT SUM(p.amount_minor) transaction_minor,
      SUM((SELECT COALESCE(SUM(CASE WHEN i.kind='receivable' THEN l.base_debit_minor-l.base_credit_minor ELSE l.base_credit_minor-l.base_debit_minor END),0)
        FROM finance_journal_lines l WHERE (l.journal_id=pj.id OR l.journal_id=rfj.id) AND l.account_id=a.id)) base_minor
    FROM finance_refund_obligations o JOIN finance_refund_instructions ri ON ri.obligation_id=o.id
    JOIN finance_payments p ON p.id=ri.payment_id AND p.company_id=i.company_id
    JOIN finance_journals pj ON pj.id=p.journal_id AND pj.company_id=i.company_id
    LEFT JOIN finance_journals rfj ON rfj.id=ri.fx_journal_id AND rfj.company_id=i.company_id AND rfj.status IN ('posted','reversed') AND rfj.entry_date<=h.as_of
    WHERE o.invoice_id=i.id AND pj.status IN ('posted','reversed') AND pj.entry_date<=h.as_of
  ) refunds ON true
  LEFT JOIN LATERAL (
    SELECT SUM(l.adjustment_minor) base_minor FROM finance_fx_revaluation_lines l
    JOIN finance_fx_revaluation_runs r ON r.id=l.run_id AND r.company_id=i.company_id
    JOIN finance_periods rp ON rp.id=r.period_id
    WHERE l.invoice_id=i.id AND r.status='posted' AND rp.end_date<=h.as_of
  ) revaluation ON true
  WHERE COALESCE(j.entry_date,i.invoice_date)<=h.as_of
    AND (j.status IN ('posted','reversed') OR (j.id IS NULL AND i.status IN ('posted','partially_paid','paid')))
)`;
