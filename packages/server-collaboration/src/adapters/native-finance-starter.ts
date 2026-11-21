import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance starter statements. */
export function nativeFinanceStarter(db: SqlDatabase, companyId: string) {
  const call = (kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return {
    chart: (v: unknown[]) => call('finance_starter_chart', v),
    account: (v: unknown[]) => call('finance_starter_account', v),
    period: (v: unknown[]) => call('finance_starter_period', v),
    dimension: (v: unknown[]) => call('finance_starter_dimension', v),
    dimensionValue: (v: unknown[]) => call('finance_starter_dimension_value', v),
  };
}
