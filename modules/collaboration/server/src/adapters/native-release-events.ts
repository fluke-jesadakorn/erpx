import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

type Facts = {
  key: string; companyId: string; releaseId: string; version: number;
  userId: string; detail: string; now: number;
};

/** Native-owned release acceptance lifecycle event statements. */
export function nativeReleaseEvents(db: SqlDatabase) {
  const call = (kind: string, facts: Facts): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, facts.companyId,
      { userId: facts.userId, recordType: '', recordId: facts.releaseId, recordVersion: facts.version, message: facts.detail, kind },
      `release-event-${facts.key}`, facts.now);
  return {
    submitted: (f: Facts) => call('release_event_submitted', f),
    reviewed: (f: Facts) => call('release_event_reviewed', f),
    revoked: (f: Facts) => call('release_event_revoked', f),
  };
}
