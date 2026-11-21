import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance PMS-import statements. */
export function financePmsImport(db: SqlDatabase, companyId: string) {
  const call = (kind: string, key: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, key, 0);
  return {
    party: (f: { partyId: string; companyId: string; code: string; name: string; now: number; now2: number }) =>
      call('finance_pms_party', '', [f.partyId, f.companyId, f.code, f.name, f.now, f.now2]),
    payment: (f: {
      paymentId: string; companyId: string; partyId: string; paymentNumber: string; cutoverDate: string;
      total: number; journalId: string | null; createdBy: string; approvedBy: string; now: number; now2: number;
    }) => call('finance_pms_payment', '',
      [f.paymentId, f.companyId, f.partyId, f.paymentNumber, f.cutoverDate, f.total, f.journalId, f.createdBy, f.approvedBy, f.now, f.now2]),
  };
}
