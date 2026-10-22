import "server-only";

/** Closed owner identities for retained Payroll ledger history. Adjustments
 * and final pay post through the same run posting. Payment batches also serve
 * other owners, so only an actual Payroll batch, run and item are included.
 * Payroll does not own generic Finance invoices or payments by source label. */
export const payrollFinanceRecordSources = {
  journal: `SELECT r.company_id,p.journal_id id
    FROM hr_payroll_postings p JOIN hr_payroll_runs r ON r.id=p.run_id
    WHERE r.status='posted'
    UNION SELECT s.company_id,s.journal_id id
    FROM hr_payroll_payment_settlements s
      JOIN hr_payment_batches b ON b.id=s.batch_id AND b.company_id=s.company_id AND b.source_type='payroll'
      JOIN hr_payroll_runs r ON r.id=b.source_id AND r.company_id=b.company_id AND r.status='posted'
      JOIN hr_payment_batch_items i ON i.id=s.batch_item_id AND i.batch_id=b.id AND i.status='settled'
    UNION SELECT s.company_id,s.journal_id id
    FROM hr_payroll_statutory_remittances s
      JOIN hr_payroll_runs r ON r.id=s.run_id AND r.company_id=s.company_id AND r.status='posted'
    UNION SELECT s.company_id,s.journal_id id
    FROM hr_payroll_recoveries s
      JOIN hr_payroll_runs r ON r.id=s.original_run_id AND r.company_id=s.company_id AND r.status='posted'
      JOIN hr_payroll_items i ON i.id=s.original_item_id AND i.run_id=r.id AND i.engagement_id=s.engagement_id AND i.status='posted'
    WHERE s.status='recognized' AND s.journal_id IS NOT NULL`,
} as const;

/** Query-free native Payroll relationships; this never purchases Payroll. */
export function payrollFinanceRecordSourceReadLocks(database:import("@erpx/server-runtime/public/runtime/types").SqlDatabase){return database.prepare('LOCK TABLE hr_payroll_postings,hr_payroll_runs,hr_payroll_payment_settlements,hr_payment_batches,hr_payment_batch_items,hr_payroll_statutory_remittances,hr_payroll_recoveries,hr_payroll_items IN SHARE ROW EXCLUSIVE MODE');}

/** These immutable native sources retain the same exact Company/run/item
 * conditions as the public identity reader, with their full row witnesses. */
export const payrollFinanceRecordSourceImages={journal:`SELECT r.company_id,p.journal_id id,jsonb_build_object('posting',to_jsonb(p),'run',to_jsonb(r)) source_image FROM hr_payroll_postings p JOIN hr_payroll_runs r ON r.id=p.run_id WHERE r.status='posted'
 UNION ALL SELECT s.company_id,s.journal_id,jsonb_build_object('settlement',to_jsonb(s),'batch',to_jsonb(b),'run',to_jsonb(r),'item',to_jsonb(i)) FROM hr_payroll_payment_settlements s JOIN hr_payment_batches b ON b.id=s.batch_id AND b.company_id=s.company_id AND b.source_type='payroll' JOIN hr_payroll_runs r ON r.id=b.source_id AND r.company_id=b.company_id AND r.status='posted' JOIN hr_payment_batch_items i ON i.id=s.batch_item_id AND i.batch_id=b.id AND i.status='settled'
 UNION ALL SELECT s.company_id,s.journal_id,jsonb_build_object('remittance',to_jsonb(s),'run',to_jsonb(r)) FROM hr_payroll_statutory_remittances s JOIN hr_payroll_runs r ON r.id=s.run_id AND r.company_id=s.company_id AND r.status='posted'
 UNION ALL SELECT s.company_id,s.journal_id,jsonb_build_object('recovery',to_jsonb(s),'run',to_jsonb(r),'item',to_jsonb(i)) FROM hr_payroll_recoveries s JOIN hr_payroll_runs r ON r.id=s.original_run_id AND r.company_id=s.company_id AND r.status='posted' JOIN hr_payroll_items i ON i.id=s.original_item_id AND i.run_id=r.id AND i.engagement_id=s.engagement_id AND i.status='posted' WHERE s.status='recognized' AND s.journal_id IS NOT NULL`} as const;
