import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance withholding remittance statements. */
export function nativeFinanceWhRemittance(db: SqlDatabase, companyId: string) {
  const call = (kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return {
    packageInsert: (v: unknown[]) => call('wh_package_insert', v),
    packageDecision: (v: unknown[]) => call('wh_package_decision', v),
    remittanceInsert: (v: unknown[]) => call('wh_remittance_insert', v),
    remittanceItemInsert: (v: unknown[]) => call('wh_remittance_item_insert', v),
    bankMatch: (v: unknown[]) => call('wh_bank_match', v),
    packageSettle: (v: unknown[]) => call('wh_package_settle', v),
    remittanceReverse: (v: unknown[]) => call('wh_remittance_reverse', v),
    packageReverse: (v: unknown[]) => call('wh_package_reverse', v),
    bankMatch2: (v: unknown[]) => call('wh_bank_match2', v),
  };
}
