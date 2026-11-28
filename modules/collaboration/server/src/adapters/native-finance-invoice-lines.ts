import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance invoice line-version statement. */
export function financeInvoiceLines(db: SqlDatabase, companyId: string) {
  return {
    version: (id: string, values: unknown[]): Promise<SqlStatement> =>
      nativeSystemNotificationStatement(db, companyId,
        { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind: 'finance_invoice_line_version', values_json: JSON.stringify(values) }, id, 0),
  };
}
