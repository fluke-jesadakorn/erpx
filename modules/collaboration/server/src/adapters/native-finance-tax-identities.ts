import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance tax identity statements. */
export function financeTaxIdentities(db: SqlDatabase, companyId: string) {
  const call = (kind: string, key: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, key, 0);
  return {
    upsert: (f: { id: string; companyId: string; values: unknown[]; userId: string; now: number; isInsert: boolean }) =>
      call(f.isInsert ? 'finance_tax_identity_insert' : 'finance_tax_identity_update', f.id,
        f.isInsert ? [f.id, f.companyId, ...f.values, f.userId, f.now, f.now] : [...f.values, f.now, f.id]),
    supersede: (f: { now: number; previousId: string }) =>
      call('finance_tax_identity_supersede', '', [f.now, f.previousId]),
    decide: (f: {
      status: string; userId: string; reason: string; now: number;
      linkId: string | null; sha256: string | null; mediaType: string | null; dataUri: string | null; id: string;
    }) => call('finance_tax_identity_decide', '',
      [f.status, f.userId, f.reason, f.now, f.linkId, f.sha256, f.mediaType, f.dataUri, f.now, f.id]),
  };
}
