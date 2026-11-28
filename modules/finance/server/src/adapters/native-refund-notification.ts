import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import { nativeNotificationStatement } from '@erpx/collaboration-server/adapters/native-notification-statement';

export type RefundAudience = 'approval' | 'treasury' | 'review' | 'completed';

/** Native-owned Finance refund notification statement. */
export function nativeRefundNotification(
  db: SqlDatabase,
  actor: WorkContext,
  key: string,
  id: string,
  audience: RefundAudience,
  message: string,
  actorId: string,
): Promise<SqlStatement> {
  return nativeNotificationStatement(
    db,
    actor,
    {
      companyId: actor.companyId,
      userId: actorId,
      recordType: 'refund',
      recordId: id,
      recordVersion: 1,
      message,
      kind: 'finance_refund',
      audience,
    },
    key,
    Date.now(),
  );
}
