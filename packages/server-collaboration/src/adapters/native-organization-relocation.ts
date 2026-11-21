import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned organization department relocation statements. */
export function nativeOrganizationRelocation(db: SqlDatabase) {
  const call = (companyId: string, kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return {
    engagement: (f: { companyId: string; branchId: string | null; nodeId: string; now: number; sourceId: string | null }) =>
      call(f.companyId, 'people_org_relocation_engagement', [f.branchId, f.nodeId, f.now, f.companyId, f.sourceId]),
    employee: (f: { companyId: string; branchId: string | null; nodeId: string; sourceId: string | null }) =>
      call(f.companyId, 'people_org_relocation_employee', [f.branchId, f.nodeId, f.companyId, f.sourceId]),
  };
}
