import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

export async function lockLegacyPeopleAccess(db: SqlDatabase) {
  await db.prepare(`LOCK TABLE hr_people,hr_engagements,people_access_cutoffs,people_lifecycle_cases,people_lifecycle_grants,
    people_package_revisions,people_applied_packages,people_provision_operations,people_provision_grants IN SHARE MODE`).run();
}

/** These are the clocks consumed by IAM's existing lifecycle/provisioning predicates. */
export async function nextLegacyPeopleTransition(db: SqlDatabase, companyId: string, observedAt: number) {
  const row = await db.prepare(`WITH boundaries AS (
    SELECT effective_at AS stamp FROM people_access_cutoffs WHERE company_id=? AND cancelled_at IS NULL
    UNION ALL SELECT restored_at FROM people_access_cutoffs WHERE company_id=? AND cancelled_at IS NULL
    UNION ALL SELECT effective_at FROM people_lifecycle_cases WHERE company_id=? AND status IN ('scheduled','completed')
    UNION ALL SELECT valid_from FROM people_applied_packages WHERE company_id=? AND ended_at IS NULL
    UNION ALL SELECT valid_until FROM people_applied_packages WHERE company_id=? AND ended_at IS NULL
  ) SELECT min(stamp) AS deadline FROM boundaries WHERE stamp>?`)
    .bind(companyId, companyId, companyId, companyId, companyId, observedAt).first<{ deadline: number | null }>();
  return row?.deadline === null || row?.deadline === undefined ? null : Number(row.deadline);
}
