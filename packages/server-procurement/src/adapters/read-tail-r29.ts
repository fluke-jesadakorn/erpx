/** Procurement read-tail r29 shared fragment. Text only: each builder renders the
 * exact original statement so binds and writes are unchanged. r17-r28 helpers,
 * locks and command_receipts are untouched. */
/** Invoice-policy version guard: absent renders NOT EXISTS without a version
 * predicate; otherwise EXISTS keyed by company and version. */
export const policyVersionGuard = (absent: boolean) =>
  `${absent ? "NOT " : ""}EXISTS(SELECT 1 FROM procurement_invoice_policies WHERE company_id=?${absent ? "" : " AND version=?"})`;
