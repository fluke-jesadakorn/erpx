import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned PMS finance fact statements. */
export function nativePmsFinanceFacts(db: SqlDatabase, companyId: string) {
  const call = (kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return {
    party: (v: unknown[]) => call('pms_finance_party', v),
    invoice: (v: unknown[]) => call('pms_finance_invoice', v),
    payment: (v: unknown[]) => call('pms_finance_payment', v),
    allocationDelete: (v: unknown[]) => call('pms_finance_allocation_delete', v),
    invoiceUpdate: (v: unknown[]) => call('pms_finance_invoice_update', v),
    allocationInsert: (v: unknown[]) => call('pms_finance_allocation_insert', v),
    bankMatch: (v: unknown[]) => call('pms_finance_bank_match', v),
  };
}
