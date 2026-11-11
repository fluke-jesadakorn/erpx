import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";

/** Files-owned route fact: the exact target of a scoped attachment set.
 * Returns `null` for missing/foreign sets so the retained route can produce
 * its own not-found response. */
export async function projectAttachmentTarget(
  db: SqlDatabase,
  companyId: string,
  setId: string,
): Promise<"project" | null> {
  if (!setId || setId.includes("\u0000")) return null;
  const row = await db
    .prepare("SELECT target_type FROM attachment_sets WHERE id=? AND company_id=?")
    .bind(setId, companyId)
    .first<{ target_type: string }>();
  return row?.target_type === "project" ? "project" : null;
}
