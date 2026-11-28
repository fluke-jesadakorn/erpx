import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import { nativeNotificationStatement } from '@erpx/collaboration-server/adapters/native-notification-statement';

/** Native-owned procurement RFQ notification statement. */
export function nativeRfqNotification(
  db: SqlDatabase,
  actor: WorkContext,
  id: string,
  key: string,
): Promise<SqlStatement> {
  return nativeNotificationStatement(
    db,
    actor,
    {
      companyId: actor.companyId,
      userId: '',
      recordType: 'procurement_rfq',
      recordId: id,
      recordVersion: 1,
      message: '',
      kind: 'procurement_rfq',
      rfq_id: id,
    },
    key,
    Date.now(),
  );
}
