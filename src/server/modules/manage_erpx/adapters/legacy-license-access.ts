import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";

export async function lockLegacyBetaAccess(db: SqlDatabase) {
  await db.prepare("LOCK TABLE beta_policy IN SHARE MODE").run();
}
