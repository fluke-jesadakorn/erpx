import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeHoldingNotificationStatement } from './native-system-notification';

/** Native-owned beta AI reservation expiry statements (holding-keyed worker). */
export function nativeBetaResourceCleanup(db: SqlDatabase, holdingId: string) {
  const call = (kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeHoldingNotificationStatement(db, holdingId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return {
    usage: (now: number) => call('beta_ai_usage_expire', [now]),
    draft: (pendingBefore: number, now: number) => call('beta_ai_draft_expire', [pendingBefore, now]),
  };
}
