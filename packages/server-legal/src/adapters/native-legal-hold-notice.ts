import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { nativeNotificationStatement } from '@erpx/server-collaboration/adapters/native-notification-statement';

/** Native-owned legal preservation-hold notification CTE statement. */
export function nativeLegalHoldNotice(
  db: SqlDatabase,
  actor: WorkContext,
  timestamp: number,
): Promise<SqlStatement> {
  return nativeNotificationStatement(
    db,
    actor,
    {
      companyId: actor.companyId,
      userId: '',
      recordType: 'legal',
      recordId: '',
      recordVersion: 1,
      message: '',
      kind: 'legal_hold',
      timestamp,
    },
    `legal-hold-${timestamp}`,
    timestamp,
  );
}
