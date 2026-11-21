import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned people statements (batch C). */
export function nativePeopleBatchC(db: SqlDatabase) {
  const call = (companyId: string, kind: string, values: unknown[], table_name?: string): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values), table_name }, '', 0);
  return {
    remittance: (f: { companyId: string; values: unknown[] }) => call(f.companyId, 'people_statutory_remittance', f.values),
    assignLevel: (f: { companyId: string; levelId: string; now: number; id: string }) =>
      call(f.companyId, 'people_assign_level', [f.levelId, f.now, f.id])
  };
}
