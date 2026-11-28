import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import {activeNodeByKind} from './organization-column-alias-families-r27';

/** Called after account access has selected this exact active Company. */
export async function productCompany(db: SqlDatabase, companyId: string) {
  return db.prepare(activeNodeByKind('id,display_name AS name','company'))
    .bind(companyId).first<{ id: string; name: string }>();
}
