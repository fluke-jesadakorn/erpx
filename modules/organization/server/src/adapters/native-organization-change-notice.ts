import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import { nativeNotificationStatement } from '@erpx/collaboration-server/adapters/native-notification-statement';

export type OrgChangeEvent = 'submitted' | 'applied' | 'rejected' | 'cancelled';

/** Native-owned organization-change notification statement. */
export function nativeOrganizationChangeNotice(
  db: SqlDatabase,
  actor: WorkContext,
  id: string,
  key: string,
  event: OrgChangeEvent,
): Promise<SqlStatement> {
  return nativeNotificationStatement(
    db,
    actor,
    {
      companyId: actor.companyId,
      userId: '',
      recordType: 'organization_change',
      recordId: id,
      recordVersion: 1,
      message: '',
      kind: 'organization_change',
      event,
    },
    key,
    Date.now(),
  );
}
