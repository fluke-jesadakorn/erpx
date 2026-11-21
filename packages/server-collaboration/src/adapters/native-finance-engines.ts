import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance calculation statements. */
export function nativeFinanceEngines(db: SqlDatabase, companyId: string) {
  const call = (kind: string, values: unknown[], table_name?: string): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values), table_name }, '', 0);
  return {
    runPrepare: (v: unknown[]) => call('finance_calc_run_prepare', v),
    eventPrepare: (v: unknown[]) => call('finance_calc_event_prepare', v),
    runDecide: (v: unknown[], column: string) => call('finance_calc_run_decide', v, column),
    eventDecide: (v: unknown[]) => call('finance_calc_event_decide', v),
    closeCheck: (v: unknown[]) => call('finance_close_check', v),
  };
}
