import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import { nativeNotificationStatement } from '@erpx/collaboration-server/adapters/native-notification-statement';

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
