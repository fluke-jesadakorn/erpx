import 'server-only';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeHoldingNotificationStatement } from './native-system-notification';

/** Native-owned beta cleanup statements. */
export function betaUploadCleanup(db: SqlDatabase, holdingId: string) {
  return {
    async expiredUploads(before: number, limit: number) {
      return (await db.prepare("SELECT id,revision FROM upload_intents WHERE status IN ('pending','uploaded') AND expires_at<? ORDER BY expires_at LIMIT ?").bind(before, limit).all<{ id: string; revision: number }>()).results;
    },
    async uploadHasAttachment(id: string) {
      return !!await db.prepare('SELECT 1 FROM attachments WHERE upload_id=?').bind(id).first();
    },
    async expireUpload(id: string, before: number, now: number): Promise<void> {
      const stmt = await nativeHoldingNotificationStatement(db, holdingId,
        { userId: '', recordType: '', recordId: id, recordVersion: 0, message: '', kind: 'beta_expire_upload', timestamp: before }, '', now);
      await stmt.run();
    },
  };
}
