import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { nativeNotificationStatement } from './native-notification-statement';

/** Native-owned Workflow next-action notice statement. The fixed document table
 * and recipient expression come from the native Workflow planning composition. */
export function stageWorkflowNotification(
  db: SqlDatabase,
  actor: WorkContext,
  input: {
    id: string;
    kind: string;
    now: number;
    companyId: string;
    recordId: string;
    actorId: string;
    documentTable: string;
    recipientSql: string;
  },
): Promise<SqlStatement> {
  return nativeNotificationStatement(
    db,
    actor,
    {
      companyId: input.companyId,
      userId: input.actorId,
      recordType: input.kind,
      recordId: input.recordId,
      recordVersion: 1,
      message: '',
      kind: 'workflow_notification',
      document_table: input.documentTable,
      recipient_sql: input.recipientSql,
    },
    input.id,
    input.now,
  );
}
