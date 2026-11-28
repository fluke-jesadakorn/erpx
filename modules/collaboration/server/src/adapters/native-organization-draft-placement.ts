import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned organization draft placement statements. */
export function nativeOrganizationDraftPlacement(db: SqlDatabase, companyId: string) {
  const call = (kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return {
    engagement: (v: unknown[]) => call('people_draft_engagement', v),
    employee: (v: unknown[]) => call('people_draft_employee', v),
    history: (v: unknown[]) => call('people_draft_history', v),
  };
}
