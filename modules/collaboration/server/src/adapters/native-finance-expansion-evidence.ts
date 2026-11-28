import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance expansion review-file statement. */
export function financeExpansionEvidence(db: SqlDatabase, companyId: string) {
  return {
    reviewFile: (f: {
      id: string; companyId: string; targetType: string; targetId: string; targetVersion: number;
      commandId: string; manifest: unknown; hash: string; userId: string; createdAt: number;
    }): Promise<SqlStatement> => nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind: 'finance_expansion_review_file',
        values_json: JSON.stringify([f.id, f.companyId, f.targetType, f.targetId, f.targetVersion, f.commandId,
          JSON.stringify(f.manifest), f.hash, f.userId, f.createdAt]) }, f.id, 0),
  };
}
