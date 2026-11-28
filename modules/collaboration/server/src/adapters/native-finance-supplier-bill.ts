import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance supplier-bill statement. */
export function financeSupplierBill(db: SqlDatabase, companyId: string) {
  return {
    voidDraft: (f: { now: number; invoiceId: string }): Promise<SqlStatement> =>
      nativeSystemNotificationStatement(db, companyId,
        { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind: 'finance_supplier_bill_draft_void',
          values_json: JSON.stringify([f.now, f.invoiceId]) }, '', 0),
  };
}
