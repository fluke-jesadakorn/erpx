import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned workflow command-assertion cleanup statement. */
export function nativeClearCommandAssertions(db: SqlDatabase, companyId: string, commandId: string): Promise<SqlStatement> {
  return nativeSystemNotificationStatement(db, companyId,
    { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind: 'workflow_command_assertion_clear',
      values_json: JSON.stringify([commandId]) }, commandId, 0);
}

/** Native-owned workflow command-assertion batch cleanup statement. */
export function nativeClearCommandAssertionsMany(db: SqlDatabase, companyId: string, keys: string[]): Promise<SqlStatement> {
  return nativeSystemNotificationStatement(db, companyId,
    { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind: 'workflow_command_assertion_clear_many',
      values_json: JSON.stringify(keys) }, keys[0] ?? '', 0);
}
