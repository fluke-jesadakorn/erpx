import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance asset statements. */
export function nativeFinanceAssets(db: SqlDatabase, companyId: string) {
  const call = (kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return {
    eventInsert: (v: unknown[]) => call('asset_event_insert', v),
    assetInsert: (v: unknown[]) => call('asset_insert', v),
    assetUpdate: (v: unknown[]) => call('asset_update', v),
    assetRetire: (v: unknown[]) => call('asset_retire', v),
    assetActivate: (v: unknown[]) => call('asset_activate', v),
    depreciationInsert: (v: unknown[]) => call('asset_depreciation_insert', v),
    depreciationUpdate: (v: unknown[]) => call('asset_depreciation_update', v),
    disposalInsert: (v: unknown[]) => call('asset_disposal_insert', v),
    disposalRetire: (v: unknown[]) => call('asset_disposal_retire', v),
    disposalDecision: (v: unknown[]) => call('asset_disposal_decision', v),
  };
}
