import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned hire handoff statements. */
export function nativePeopleHandoffs(db: SqlDatabase) {
  const call = (companyId: string, kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return {
    person: (c: string, v: unknown[]) => call(c, 'people_handoff_person', v),
    engagement: (c: string, v: unknown[]) => call(c, 'people_handoff_engagement', v),
    compensation: (c: string, v: unknown[]) => call(c, 'people_handoff_compensation', v),
  };
}
