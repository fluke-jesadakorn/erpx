import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { nativeNotificationStatement } from './native-notification-statement';

/** Native-owned Workflow goods-movement invoicing notice statement. */
export function stageWorkflowInvoicingNotification(
  db: SqlDatabase,
  actor: WorkContext,
  id: string,
  now: number,
  journalId: string,
): Promise<SqlStatement> {
  return nativeNotificationStatement(
    db,
    actor,
    {
      companyId: actor.companyId,
      userId: '',
      recordType: 'order',
      recordId: journalId,
      recordVersion: 1,
      message: '',
      kind: 'workflow_invoicing',
    },
    id,
    now,
  );
}
