/** Procurement read-tail r20 shared fragments. Read-only text only: each builder
 * renders the exact original statement so binds, ordering, locks and writes are
 * unchanged. The r17/r18/r19 helpers stay untouched. */
export const receiptCount = (ref: string) =>
  `(SELECT count(*) FROM procurement_bill_match_receipts br WHERE br.match_id=${ref}) receipt_count`;
/** Expense account by id and chart; the matching-mode guard reuses this filter. */
export const expenseAccountBy = (accounts: string, charts: string, columns: string) =>
  `SELECT ${columns} FROM (${accounts}) a JOIN (${charts}) c ON c.id=a.chart_id AND c.company_id=a.company_id WHERE a.id=? AND a.company_id=? AND a.chart_id=? AND a.currency=c.base_currency AND a.category='expense' AND a.status='active'`;
/** HR person-in-company scope shared by the calendar fact reads. */
export const hrPersonIn = (hrPeople: string, userColumn: string, companyAlias: string) =>
  `${userColumn} IN(SELECT user_id FROM (${hrPeople}) hr_people WHERE id=? AND company_id=${companyAlias}.company_id)`;
