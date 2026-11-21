import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { nativeNotificationStatement } from './native-notification-statement';

/** Native-owned official law-source change notice statement. */
export function legalSourceNotification(
  db: SqlDatabase,
  actor: WorkContext,
  input: { identifier: string; now: number; ruleId: string; unchanged: boolean },
): Promise<SqlStatement> {
  return nativeNotificationStatement(
    db,
    actor,
    {
      companyId: actor.companyId,
      userId: '',
      recordType: 'legal',
      recordId: input.ruleId,
      recordVersion: 1,
      message: '',
      kind: 'legal_law_source',
      identifier: input.identifier,
      item_id: input.ruleId,
      event: input.unchanged ? 'unchanged' : 'changed',
    },
    input.identifier,
    input.now,
  );
}
