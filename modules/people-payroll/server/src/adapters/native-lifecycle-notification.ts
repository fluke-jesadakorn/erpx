import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import { nativeNotificationStatement } from '@erpx/collaboration-server/adapters/native-notification-statement';

/** Native-owned retirement-clearance notification statement. */
export function nativeLifecycleNotification(
  db: SqlDatabase,
  actor: WorkContext,
  facts: { value: string; companyId: string; owner: string; taskId: string; value2: string; now: number },
): Promise<SqlStatement> {
  return nativeNotificationStatement(
    db,
    actor,
    {
      companyId: facts.companyId,
      userId: facts.owner,
      recordType: 'retirement_clearance',
      recordId: facts.taskId,
      recordVersion: 1,
      message: facts.value2,
      kind: 'retirement_clearance',
    },
    facts.value,
    facts.now,
  );
}
