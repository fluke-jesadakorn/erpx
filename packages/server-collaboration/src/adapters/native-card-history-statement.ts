import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { Row } from '@erpx/server-collaboration/public/collaboration-work';
import { nativeNotificationStatement } from './native-notification-statement';

/** Native-owned card-history read statement. The SQL text and scope binds are
 * issued by the Collaboration owner against the authorized session selection;
 * the caller only supplies the card id and executes the statement inside its
 * own serializable unit, so uncommitted notification work stays observable and
 * out-of-scope cards yield no rows instead of leaking foreign events. */
export async function nativeCardHistoryStatement(
  db: SqlDatabase,
  actor: WorkContext,
  cardId: string,
  key: string,
  now: number,
): Promise<{ results: Row[] }> {
  const statement = await nativeNotificationStatement(
    db,
    actor,
    {
      companyId: actor.companyId,
      userId: actor.userId,
      recordType: 'collaboration_action_card',
      recordId: cardId,
      recordVersion: 1,
      message: 'card history read',
      kind: 'collaboration_card_history',
    },
    key,
    now,
  );
  return statement.all<Row>();
}
