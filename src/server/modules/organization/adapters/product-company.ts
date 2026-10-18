import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";

/** Called after account access has selected this exact active Company. */
export async function productCompany(db: SqlDatabase, companyId: string) {
  return db.prepare("SELECT id,display_name AS name FROM organization_nodes WHERE id=? AND kind='company' AND status='active'")
    .bind(companyId).first<{ id: string; name: string }>();
}
