import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned workflow command-assertion cleanup statement. */
export function nativeClearCommandAssertions(db: SqlDatabase, companyId: string, commandId: string): Promise<SqlStatement> {
  return nativeSystemNotificationStatement(db, companyId,
    { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind: 'workflow_command_assertion_clear',
      values_json: JSON.stringify([commandId]) }, commandId, 0);
}
