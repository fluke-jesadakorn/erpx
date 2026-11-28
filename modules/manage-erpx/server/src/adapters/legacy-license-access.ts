import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";

export async function lockLegacyBetaAccess(db: SqlDatabase) {
  await db.prepare("LOCK TABLE beta_policy IN SHARE MODE").run();
}
