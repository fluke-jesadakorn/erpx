import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance sales defaults statements. */
export function financeSalesDefaults(db: SqlDatabase, companyId: string) {
  const call = (kind: string, key: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, key, 0);
  return {
    create: (f: {
      id: string; companyId: string; chartId: string; currency: string;
      receivableAccountId: string; revenueAccountId: string; taxAccountId: string | null;
      reason: string; userId: string; now: number;
    }) => call('finance_sales_defaults_create', f.id,
      [f.id, f.companyId, f.chartId, f.currency, f.receivableAccountId, f.revenueAccountId, f.taxAccountId, f.reason, f.userId, f.now, f.now]),
    supersede: (f: { now: number; companyId: string; currency: string }) =>
      call('finance_sales_defaults_supersede', '', [f.now, f.companyId, f.currency]),
    decide: (f: { status: string; userId: string; reason: string; now: number; id: string }) =>
      call('finance_sales_defaults_decide', '', [f.status, f.userId, f.reason, f.now, f.id]),
  };
}
