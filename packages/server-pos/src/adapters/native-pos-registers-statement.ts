import 'server-only';
import { randomUUID } from 'node:crypto';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { nativeNotificationStatement } from '@erpx/server-collaboration/adapters/native-notification-statement';
import type { PosRow } from '@erpx/server-pos/domain/pos-model';

/** Owner-issued POS registers list for caller-owned workspace transactions.
 * The SQL text (with the commerce warehouse fragment inlined byte-identically)
 * and scope binds are issued by the native owner against the authorized
 * session selection; the caller executes the statement inside its own
 * serializable unit, so uncommitted workspace work stays observable and
 * out-of-scope rows never leak. */
export async function nativePosRegistersStatement(db: SqlDatabase, actor: WorkContext): Promise<PosRow[]> {
  const statement = await nativeNotificationStatement(
    db,
    actor,
    {
      companyId: actor.companyId,
      userId: actor.userId,
      recordType: 'pos_register',
      recordId: 'registers',
      recordVersion: 1,
      message: 'pos registers read',
      kind: 'pos_registers',
    },
    randomUUID(),
    Date.now(),
  );
  return (await statement.all<PosRow>()).results;
}

/** Owner-issued POS sale row. Company scoping stays with the caller's
 * scopeRow exactly like the retained read. */
export async function nativePosSaleStatement(db: SqlDatabase, actor: WorkContext, saleId: string): Promise<PosRow | null> {
  const statement = await nativeNotificationStatement(
    db,
    actor,
    {
      companyId: actor.companyId,
      userId: actor.userId,
      recordType: 'pos_sale',
      recordId: saleId,
      recordVersion: 1,
      message: 'pos sale read',
      kind: 'pos_sale',
    },
    randomUUID(),
    Date.now(),
  );
  return statement.first<PosRow>();
}
