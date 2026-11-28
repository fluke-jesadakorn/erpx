/** G4 / retire.finance.read-tail-r43 safe NEW read-tail SQL families.
 *
 * Every export renders byte-identical text to the literal it replaces. Callers
 * keep their own binds, result types and transaction; nothing here is cached or
 * executed. Row-lock, tax, assertion, native and r15-r42 helpers are untouched.
 */

export const journalLineCountSql = (journalRef: string) => `SELECT count(*) FROM finance_journal_lines line WHERE line.journal_id=${journalRef}`;

export const journalLineNetSql = (journalRef: string) => `SELECT sum(line.debit_minor-line.credit_minor) FROM finance_journal_lines line WHERE line.journal_id=${journalRef}`;

export const BOOKS_AGGREGATE_IMAGE_SQL = 'SELECT jsonb_agg(to_jsonb(b) ORDER BY b.primary_journal_id,b.secondary_journal_id) FROM books b';

export const childBillingRunExistsSql = (parentRef: string) => `SELECT 1 FROM finance_business_billing_runs child WHERE child.parent_run_id=${parentRef}`;
