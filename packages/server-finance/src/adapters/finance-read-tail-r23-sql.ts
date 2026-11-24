/** Byte-identical Finance read-tail SQL families shared across the initial-stock
 * journal sources and People package-obligation facts. Text only: each call site
 * keeps its own binds and prepares the rendered text. Lock-taking (FOR UPDATE /
 * FOR SHARE) variants stay inline; r15-r22 helpers are untouched. */

export const bookRootUnionSql = (alias: string) => `SELECT ${alias}.id,${alias}.company_id UNION SELECT book.primary_journal_id,${alias}.company_id FROM finance_book_postings book WHERE book.secondary_journal_id=${alias}.id AND book.company_id=${alias}.company_id`;

export const peopleRunsCteSql = (runs: string) => `runs AS MATERIALIZED(SELECT r.* FROM (${runs}) r,scope WHERE r.company_id=scope.company_id),`;

export const peoplePostingsCteSql = (postings: string) => `postings AS MATERIALIZED(SELECT p.*,r.company_id,r.currency FROM (${postings}) p JOIN runs r ON r.id=p.run_id),`;

export const journalAggregateCtesSql = () => `journal_ids AS MATERIALIZED(SELECT id FROM owned_journals UNION SELECT book.secondary_journal_id FROM finance_book_postings book JOIN owned_journals owned ON owned.id=book.primary_journal_id),
 journals AS MATERIALIZED(SELECT j.*,COALESCE((SELECT SUM(l.debit_minor-l.credit_minor) FROM finance_journal_lines l WHERE l.journal_id=j.id),0) imbalance,
 (SELECT COUNT(*) FROM finance_journal_lines l WHERE l.journal_id=j.id) line_count,COALESCE((SELECT SUM(l.debit_minor) FROM finance_journal_lines l WHERE l.journal_id=j.id),0) debit_total`;

export const netPayableFactSql = (items: string,batchItems: string) => `UNION ALL SELECT 'net-payable',r.id,r.version,r.status,to_jsonb(r),(SELECT COALESCE(SUM(i.net_minor),0) FROM (${items}) i WHERE i.run_id=r.id AND i.status='posted')<>COALESCE((SELECT SUM(item.amount_minor) FROM batches b JOIN (${batchItems}) item ON item.batch_id=b.id WHERE b.source_type='payroll' AND b.source_id=r.id AND b.status='settled' AND item.status='settled'),0) FROM runs r WHERE r.status='posted'`;

export const payrollUnpostedFactSql = () => `UNION ALL SELECT 'payroll-unposted',r.id,r.version,r.status,to_jsonb(r),r.status<>'cancelled' AND (r.status<>'posted' OR NOT EXISTS(SELECT 1 FROM postings p WHERE p.run_id=r.id)) FROM runs r`;

export const statutoryLiabilityFactSql = (remittances: string) => `UNION ALL SELECT 'statutory-liability',p.run_id||':'||line.description,1,line.description,jsonb_build_object('posting',to_jsonb(p),'accountId',line.account_id,'amount',line.amount),line.amount<>COALESCE((SELECT SUM(remittance.amount_minor) FROM (${remittances}) remittance WHERE remittance.company_id=p.company_id AND remittance.run_id=p.run_id AND remittance.kind=CASE WHEN line.description='Payroll tax' THEN 'withholding_tax' ELSE 'social_security' END),0)`;

export const statutoryLateralSql = () => `FROM postings p CROSS JOIN LATERAL(SELECT l.account_id,l.description,SUM(l.credit_minor-l.debit_minor) amount FROM finance_journal_lines l WHERE l.journal_id=p.journal_id AND l.description IN ('Payroll tax','Payroll social security') GROUP BY l.description,l.account_id) line`;

export const secondaryJournalFactSql = () => `UNION ALL SELECT 'secondary-journal',book.secondary_journal_id,book.source_version,'posted',to_jsonb(book)||jsonb_build_object('journal',to_jsonb(secondary),'rule',to_jsonb(rule)),book.company_id IS DISTINCT FROM owned.company_id OR primary_journal.id IS NULL OR secondary.id IS NULL OR secondary.status<>'posted' OR secondary.imbalance<>0 OR secondary.line_count<2 OR rule.id IS NULL OR rule.company_id IS DISTINCT FROM owned.company_id OR rule.status<>'approved' OR rule.reviewed_by IS NULL OR rule.reviewed_by=rule.prepared_by OR rule.primary_chart_id IS DISTINCT FROM primary_journal.chart_id OR rule.secondary_chart_id IS DISTINCT FROM secondary.chart_id OR secondary.currency IS DISTINCT FROM primary_journal.currency FROM owned_journals owned JOIN finance_book_postings book ON book.primary_journal_id=owned.id LEFT JOIN journals primary_journal ON primary_journal.id=owned.id LEFT JOIN journals secondary ON secondary.id=book.secondary_journal_id LEFT JOIN finance_book_rules rule ON rule.id=book.rule_id`;
