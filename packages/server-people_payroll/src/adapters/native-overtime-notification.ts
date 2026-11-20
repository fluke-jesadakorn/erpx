import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { nativeNotificationStatement } from '@erpx/server-collaboration/adapters/native-notification-statement';

/** Native-owned Overtime approval notification statement. */
export async function nativeOvertimeNotification(
  db: SqlDatabase,
  actor: WorkContext,
  scope: { company_id: string },
  userId: string,
  type: string,
  id: string,
  version: number,
  message: string,
): Promise<SqlStatement> {
  return nativeNotificationStatement(
    db,
    actor,
    {
      companyId: scope.company_id,
      userId,
      recordType: type,
      recordId: id,
      recordVersion: version,
      message,
    },
    `overtime-${id}-${version}`,
    Date.now(),
  );
}
