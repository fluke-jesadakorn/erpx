import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import { nativeNotificationStatement } from '@erpx/collaboration-server/adapters/native-notification-statement';

/** Native-owned CRM lead handoff notification statement. */
export function nativeCrmLeadNotification(
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
      recordType: 'crm_lead',
      recordId: id,
      recordVersion: 1,
      message: 'CRM handoff changed: review the source and next action',
      kind: 'crm_lead',
      leadId: id,
    },
    key,
    Date.now(),
  );
}
