import "server-only";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {nativeFilesRead} from "./native-files-command";

/** Files-owned route fact: the exact target of a scoped attachment set.
 * Returns `null` for missing/foreign sets so the retained route can produce
 * its own not-found response. The target query is owned by the native Files
 * read port (`RD_PROJECT_ATTACHMENT_TARGET`); this module carries no SQL. */
export async function projectAttachmentTarget(
  db: SqlDatabase,
  companyId: string,
  setId: string,
): Promise<"project" | null> {
  if (!setId || setId.includes("\u0000")) return null;
  const row = await nativeFilesRead<{ target_type: string }>(
    "RD_PROJECT_ATTACHMENT_TARGET",
    [setId, companyId],
    db,
    companyId,
  );
  return row?.target_type === "project" ? "project" : null;
}
