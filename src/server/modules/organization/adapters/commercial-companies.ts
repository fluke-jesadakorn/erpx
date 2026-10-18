import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";

/** Provider listing, called only after Platform Commercial authorization. */
export async function commercialCompanies(db: SqlDatabase) {
  return (await db.prepare("SELECT id,slug,display_name AS name FROM organization_nodes WHERE kind='company' AND status='active' ORDER BY display_name,id")
    .all<{ id: string; slug: string; name: string }>()).results;
}

/** Freeze hierarchy and HR appointments while a one-off legacy license review is applied. */
export async function lockLegacyOrganizationAccess(db: SqlDatabase) {
  await db.prepare("LOCK TABLE organization_nodes,authority_seats,organization_positions IN SHARE MODE").run();
}

/** A future HR appointment can change the published module ceiling by time alone. */
export async function nextLegacyOrganizationTransition(db: SqlDatabase, companyId: string, observedAt: number) {
  const row = await db.prepare("SELECT min(effective_at) AS deadline FROM authority_seats WHERE company_id=? AND kind='hr' AND scope_id=? AND effective_at>?")
    .bind(companyId, companyId, observedAt).first<{ deadline: number | null }>();
  return row?.deadline === null || row?.deadline === undefined ? null : Number(row.deadline);
}
