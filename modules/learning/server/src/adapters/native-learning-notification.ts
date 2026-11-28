import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import { nativeNotificationStatement } from '@erpx/collaboration-server/adapters/native-notification-statement';

/** Native-owned learning assignment notification statement. */
export function nativeLearningAssignmentNotice(
  db: SqlDatabase,
  actor: WorkContext,
  change: 'assigned' | 'changed',
  noticeId: string,
  id: string,
): Promise<SqlStatement> {
  return nativeNotificationStatement(
    db,
    actor,
    {
      companyId: actor.companyId,
      userId: '',
      recordType: 'learning_assignment',
      recordId: id,
      recordVersion: 1,
      message: '',
      kind: 'learning_assignment',
      change,
    },
    noticeId,
    Date.now(),
  );
}
