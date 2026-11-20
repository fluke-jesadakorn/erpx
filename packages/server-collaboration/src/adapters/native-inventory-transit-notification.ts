import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { nativeNotificationStatement } from './native-notification-statement';

/** Native-owned inventory transport handoff notice statement. */
export function inventoryTransitSources(db: SqlDatabase, actor: WorkContext) {
  return {
    prepareTransitNotice(key: string, id: string): Promise<SqlStatement> {
      return nativeNotificationStatement(
        db,
        actor,
        {
          companyId: actor.companyId,
          userId: '',
          recordType: 'inventory_transit',
          recordId: id,
          recordVersion: 1,
          message: '',
          kind: 'inventory_transit',
        },
        key,
        Date.now(),
      );
    },
  };
}
