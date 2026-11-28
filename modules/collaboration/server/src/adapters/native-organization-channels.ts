import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned organization channel provisioning statements. */
export function nativeOrganizationChannels(db: SqlDatabase, companyId: string, sourceSql: string) {
  const call = (kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values), source_access_sql: sourceSql }, '', 0);
  return {
    team: (teamAt: number) => call('collaboration_org_team', [teamAt, companyId]),
    room: (roomsAt: number) => call('collaboration_org_room', [roomsAt, companyId]),
  };
}
