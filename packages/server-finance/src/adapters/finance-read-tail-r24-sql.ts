/**
 * G4 / retire.finance.read-tail-r24 byte-identical read families.
 *
 * Every fragment below is rendered byte-for-byte as the pre-dedupe inline SQL.
 * All transaction lock and row-lock statements stay inline in the calling adapters.
 */
export type PackageObligationFactsArgs = {
  sourceType: string;
  payments: string;
  paymentFk: string;
  postingKey: string;
  postings: string;
  postingGroup: string;
  clearingFrom: string;
  clearingWhere: string;
};

export function packageObligationFactsSql(a: PackageObligationFactsArgs): string {
  return `WITH scope AS (SELECT ?::text company_id)
    SELECT 'invoice' kind,i.id,i.version,i.status state,i.outstanding_minor::text amount FROM finance_invoices i,scope
      WHERE i.company_id=scope.company_id AND i.source_type='${a.sourceType}' AND (i.outstanding_minor<>0 OR i.status NOT IN ('paid','voided'))
    UNION ALL SELECT 'payment',p.id,p.version,p.status,p.outstanding_minor::text FROM finance_payments p,scope
      WHERE p.company_id=scope.company_id AND EXISTS(SELECT 1 FROM (${a.payments}) source WHERE source.company_id=p.company_id AND source.${a.paymentFk}=p.id)
        AND (p.status NOT IN ('settled','voided') OR p.outstanding_minor<>0)
    UNION ALL SELECT 'posting',${a.postingKey},COALESCE(j.version,0),COALESCE(j.status,'missing'),COALESCE(sum(l.base_debit_minor-l.base_credit_minor),0)::text
      FROM (${a.postings}) pp LEFT JOIN finance_journals j ON j.id=pp.journal_id AND j.company_id=pp.company_id
      LEFT JOIN finance_journal_lines l ON l.journal_id=j.id,scope WHERE pp.company_id=scope.company_id
      GROUP BY ${a.postingGroup},j.id,j.version,j.status HAVING j.id IS NULL OR j.status<>'posted' OR count(l.id)<2 OR sum(l.base_debit_minor-l.base_credit_minor)<>0
    UNION ALL SELECT 'clearing',concat(l.account_id,':',j.currency),0,'unsettled',sum(l.debit_minor-l.credit_minor)::text
      ${a.clearingFrom}
      WHERE j.company_id=scope.company_id AND j.status='posted' AND ${a.clearingWhere}
      GROUP BY l.account_id,j.currency HAVING sum(l.debit_minor-l.credit_minor)<>0 ORDER BY kind,id`;
}

export const ASSET_DEPRECIATION_COUNT_SQL = "SELECT count(*) n FROM finance_asset_depreciations WHERE asset_id=?";
export const ASSET_POLICY_EDITION_PENDING_SQL = "SELECT 1 FROM finance_asset_policy_editions WHERE asset_id=? AND status='draft'";
export const ASSET_POLICY_RUN_PENDING_SQL = "SELECT 1 FROM finance_asset_policy_runs WHERE asset_id=? AND status='draft'";

export const TAX_SOURCE_APPROVED_INVOICE_SQL = "NOT EXISTS(SELECT 1 FROM finance_current_tax_entries t WHERE t.company_id=i.company_id AND t.source_type='invoice' AND t.source_id=i.id AND t.status='approved')";
export const TAX_SOURCE_APPROVED_COMMERCE_RETURN_SQL = "NOT EXISTS(SELECT 1 FROM finance_current_tax_entries t WHERE t.company_id=r.company_id AND t.source_type='commerce_return' AND t.source_id=r.id AND t.status='approved')";
export const TAX_SOURCE_APPROVED_BILLING_CREDIT_SQL = "NOT EXISTS(SELECT 1 FROM finance_current_tax_entries t WHERE t.company_id=r.company_id AND t.source_type='billing_credit' AND t.source_id=r.id AND t.status='approved')";
