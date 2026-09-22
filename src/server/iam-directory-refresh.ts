import "server-only";
import { iamDb } from "./iam-db";
import { ApiError } from "./http";
import type { PostgresDatabase } from "./runtime/postgres";
import type { ConnectionRow } from "./iam-directories";

export async function refreshDirectoryMember(
  db: PostgresDatabase,
  companyId: string,
  userId: string,
) {
  db = iamDb(db);
  const row = await db
    .prepare(
      "SELECT c.* FROM iam_members m JOIN iam_connections c ON c.id=m.connection_id JOIN iam_identities i ON i.id=m.identity_id WHERE m.company_id=? AND m.user_id=? AND m.status='active' AND m.login_policy='directory' AND c.status='active' AND i.observed_at<=?",
    )
    .bind(companyId, userId, Date.now() - 300000)
    .first<ConnectionRow>();
  if (row)
    try {
      const { synchronizeConnection } = await import("./iam-directories");
      await synchronizeConnection(db, row);
    } catch {
      throw new ApiError(
        403,
        "DIRECTORY_ACCESS_UNAVAILABLE",
        "Directory verification failed. Dependent access is unavailable until a successful refresh.",
      );
    }
}
