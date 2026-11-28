import 'server-only';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import type { Row } from '@erpx/collaboration-server/public/collaboration-work';
import { nativeNotificationStatement } from './native-notification-statement';

/** Owner-issued Collaboration read statements for caller-owned transactions.
 * The SQL text and scope binds are issued by the native owner against the
 * authorized session selection; the caller only supplies the read key and
 * executes the statement inside its own serializable unit, so uncommitted
 * work stays observable and out-of-scope rows never leak. */
export async function nativeCollaborationReadStatement(
  db: SqlDatabase,
  actor: WorkContext,
  kind: 'collaboration_card_history' | 'collaboration_record_relations' | 'collaboration_action_cards' | 'collaboration_eligible_people',
  recordId: string,
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
      recordId,
      recordVersion: 1,
      message: 'collaboration read',
      kind,
    },
    key,
    now,
  );
  return statement.all<Row>();
}

/** Native-owned card-history read statement (see above). */
export async function nativeCardHistoryStatement(
  db: SqlDatabase,
  actor: WorkContext,
  cardId: string,
  key: string,
  now: number,
): Promise<{ results: Row[] }> {
  return nativeCollaborationReadStatement(db, actor, 'collaboration_card_history', cardId, key, now);
}
