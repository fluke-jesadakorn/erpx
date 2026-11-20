import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { nativeNotificationStatement } from '@erpx/server-collaboration/adapters/native-notification-statement';

/** Native-owned supplier-bill notification statement. */
export function nativeSupplierBillNotification(
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
      recordType: 'supplier_bill',
      recordId: id,
      recordVersion: 1,
      message: '',
      kind: 'supplier_bill',
      bill_id: id,
    },
    key,
    Date.now(),
  );
}
