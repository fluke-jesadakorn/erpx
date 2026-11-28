import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeHoldingNotificationStatement } from './native-system-notification';

/** Native-owned finance group report-edition statements. */
export function financeGroupEditions(db: SqlDatabase, companyId: string) {
  const call = (kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeHoldingNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return {
    prepare: (values: unknown[]) => call('finance_group_edition_prepare', values),
    supersede: (values: unknown[]) => call('finance_group_edition_supersede', values),
    decide: (values: unknown[]) => call('finance_group_edition_decide', values),
  };
}
