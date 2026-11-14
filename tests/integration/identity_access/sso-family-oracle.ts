import type { TransactionalSqlDatabase } from "@erpx/server-runtime/public/runtime/types";

/**
 * Test oracle for the native SSO family deletion protocol.
 *
 * Production deletes a parent Account session and its bound product children in
 * Rust (`account_postgres`, used by native Account security and Account
 * revocation). These tests simulate a concurrent family deletion against the
 * retained Better Auth session tables, so the SQL lives in the test lane rather
 * than in an owner adapter.
 */
export async function revokeSsoFamily(
  database: TransactionalSqlDatabase,
  sessionId: string,
  environment: "development" = "development",
) {
  await database.transaction(async tx => {
    const row = await tx
      .prepare("SELECT * FROM sso_session_bindings WHERE session_id=?")
      .bind(sessionId)
      .first<{ parent_session_id: string | null; environment: string }>();
    if (!row || row.environment !== environment) return;
    const parent = row.parent_session_id ?? sessionId;
    await tx.prepare("SELECT id FROM session WHERE id=? FOR UPDATE").bind(parent).first();
    await tx
      .prepare(
        "DELETE FROM session WHERE id=? OR id IN (SELECT session_id FROM sso_session_bindings WHERE parent_session_id=? AND environment=?)",
      )
      .bind(parent, parent, environment)
      .run();
  });
}
