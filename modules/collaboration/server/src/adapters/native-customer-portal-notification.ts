import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import { nativeNotificationStatement } from './native-notification-statement';

/** Native-owned customer-portal staff notification statement. */
export async function notifyCustomerPortalStaff(
  db: SqlDatabase,
  actor: WorkContext,
  input: { id: string; message: string; now: number; orderId: string; companyId: string },
): Promise<void> {
  const statement = await nativeNotificationStatement(
    db,
    actor,
    {
      companyId: input.companyId,
      userId: '',
      recordType: 'commerce_order',
      recordId: input.orderId,
      recordVersion: 1,
      message: input.message,
      kind: 'customer_portal',
      order_id: input.orderId,
    },
    input.id,
    input.now,
  );
  await statement.run();
}
