import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance/POS party statement. */
export function financePosFacts(db: SqlDatabase, companyId: string) {
  return {
    party: (f: {
      customer: string; companyId: string; code: string; name: string; currency: unknown;
      phone: string | null; email: string | null; addressJson: string; createdAt: number; updatedAt: number;
    }): Promise<SqlStatement> => nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind: 'finance_pos_party',
        values_json: JSON.stringify([f.customer, f.companyId, f.code, f.name, f.currency, f.phone, f.email, f.addressJson, f.createdAt, f.updatedAt]) }, f.customer, 0),
  };
}
