import type { SqlDatabase, FileStore } from "./runtime/types";
/** Bounded cleanup: an expired intent cannot finalize, so its uncommitted object is safe to remove. */
export async function cleanupBetaResources(
  db: SqlDatabase,
  files: FileStore,
  holdingId: string,
) {
  const now = Date.now();
  const expired = await db
    .prepare(
      "SELECT id,revision FROM upload_intents WHERE status IN ('pending','uploaded') AND expires_at<? ORDER BY expires_at LIMIT 20",
    )
    .bind(now - 60000)
    .all<{ id: string; revision: number }>();
  for (const row of expired.results) {
    if (
      await db
        .prepare("SELECT 1 FROM attachments WHERE upload_id=?")
        .bind(row.id)
        .first()
    )
      continue;
    await files.delete([
      `${holdingId}/objects/${row.id}/r${row.revision}`,
      `${holdingId}/staging/${row.id}`,
    ]);
    await db
      .prepare(
        "UPDATE upload_intents SET status='expired',updated_at=? WHERE id=? AND status IN ('pending','uploaded') AND expires_at<?",
      )
      .bind(now, row.id, now - 60000)
      .run();
  }
  await db.batch([
    db
      .prepare(
        "UPDATE social_ai_usage SET state='failed' WHERE state='reserved' AND lease_until<?",
      )
      .bind(now),
    db
      .prepare(
        "UPDATE social_ai_drafts SET state='failed',result_json=NULL WHERE (state='pending' AND created_at<?) OR (state='ready' AND expires_at<?)",
      )
      .bind(now - 120000, now),
  ]);
}
