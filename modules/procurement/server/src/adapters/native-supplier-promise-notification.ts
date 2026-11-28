import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import { nativeNotificationStatement } from '@erpx/collaboration-server/adapters/native-notification-statement';

/** Native-owned supplier-promise notification statement. */
export function nativeSupplierPromiseNotification(
  db: SqlDatabase,
  actor: WorkContext,
  key: string,
  id: string,
): Promise<SqlStatement> {
  return nativeNotificationStatement(
    db,
    actor,
    {
      companyId: actor.companyId,
      userId: '',
      recordType: 'supplier_promise',
      recordId: id,
      recordVersion: 1,
      message: '',
      kind: 'supplier_promise',
      order_id: id,
    },
    key,
    Date.now(),
  );
}
