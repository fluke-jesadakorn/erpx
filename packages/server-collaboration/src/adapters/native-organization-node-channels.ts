import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned organization node-channel statements. */
export function nativeOrganizationNodeChannels(db: SqlDatabase, companyId: string, sourceSql: string) {
  const call = (kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values), source_access_sql: sourceSql }, '', 0);
  return {
    team: (now: number) => call('org_node_team', [`org-team-${companyId}`, companyId, now]),
    room: (f: { nodeId: string | null; displayName: string; now: number }) =>
      call('org_node_room', [`org-channel-${f.nodeId}`, companyId, f.displayName, `org-team-${companyId}`, f.nodeId, f.now, f.nodeId, companyId]),
    rename: (f: { displayName: string; nodeId: string | null }) => call('org_node_room_rename', [f.displayName, companyId, f.nodeId]),
  };
}
