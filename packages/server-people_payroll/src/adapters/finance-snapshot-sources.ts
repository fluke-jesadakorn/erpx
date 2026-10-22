import "server-only";
/** Closed payroll and calendar person projections retain original Company guards in one statement. */
export function financeSnapshotSources(){return {
  "payrollPostingSource": "FROM hr_payroll_postings p JOIN hr_payroll_runs r ON r.id=p.run_id",
  "refundOwner": "coalesce((SELECT legal_name FROM hr_people WHERE user_id=i.prepared_by AND company_id=o.company_id AND merged_into_id IS NULL LIMIT 1),'Treasury') owner_name",
  "refundPerson": "i.prepared_by IN (SELECT user_id FROM hr_people WHERE id=? AND company_id=o.company_id)",
  "batchPerson": "b.created_by IN (SELECT user_id FROM hr_people WHERE id=? AND company_id=b.company_id)",
  "billPerson": "r.created_by IN(SELECT user_id FROM hr_people WHERE id=? AND company_id=r.company_id)"
} as const;}
