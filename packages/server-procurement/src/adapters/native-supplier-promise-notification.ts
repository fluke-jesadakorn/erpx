import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { nativeNotificationStatement } from '@erpx/server-collaboration/adapters/native-notification-statement';

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
