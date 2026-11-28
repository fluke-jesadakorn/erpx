import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned project-worker starter statements (server-chosen tables). */
export function nativeWorkerStartup(db: SqlDatabase) {
  const call = (companyId: string, kind: string, table: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values), table_name: table }, '', 0);
  return {
    starter: (companyId: string, table: string, values: unknown[]) => call(companyId, 'people_worker_starter', table, values),
    placement: (companyId: string, table: string, values: unknown[]) => call(companyId, 'people_worker_placement', table, values),
  };
}
