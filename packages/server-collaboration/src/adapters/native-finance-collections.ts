import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance collection statements. */
export function nativeFinanceCollections(db: SqlDatabase, companyId: string) {
  const call = (kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return {
    prepare: (v: unknown[]) => call('finance_collection_prepare', v),
    supersede: (v: unknown[]) => call('finance_collection_supersede', v),
    revise: (v: unknown[]) => call('finance_collection_revise', v),
    delivery: (v: unknown[]) => call('finance_collection_delivery', v),
    decide: (v: unknown[]) => call('finance_collection_decide', v),
  };
}
