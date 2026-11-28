import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { activeCompanyNodes } from './organization-read-tail-sources-r42';
import { authoritySeatCompanyRead } from './organization-read-tail-sources-r51';

/** Provider listing, called only after Platform Commercial authorization. */
export async function commercialCompanies(db: SqlDatabase) {
  return (await db.prepare(activeCompanyNodes('id,slug,display_name AS name', ' ', 'display_name,id'))
    .all<{ id: string; slug: string; name: string }>()).results;
}

/** Freeze hierarchy and HR appointments while a one-off legacy license review is applied. */
export async function lockLegacyOrganizationAccess(db: SqlDatabase) {
  await db.prepare("LOCK TABLE organization_nodes,authority_seats,organization_positions IN SHARE MODE").run();
}

/** A future HR appointment can change the published module ceiling by time alone. */
export async function nextLegacyOrganizationTransition(db: SqlDatabase, companyId: string, observedAt: number) {
  const row = await db.prepare(authoritySeatCompanyRead('min(effective_at) AS deadline', "kind='hr' AND scope_id=? AND effective_at>?"))
    .bind(companyId, companyId, observedAt).first<{ deadline: number | null }>();
  return row?.deadline === null || row?.deadline === undefined ? null : Number(row.deadline);
}
