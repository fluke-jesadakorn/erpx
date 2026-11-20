import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { nativeNotificationStatement } from '@erpx/server-collaboration/adapters/native-notification-statement';

type Facts = { value1: string; value2: string; value3: string; value4: string; value5: number; value6: string; value7: number };

/** Native-owned calendar retirement-clearance notice statement. */
export function nativeCalendarRetirementNotification(
  db: SqlDatabase,
  actor: WorkContext,
  facts: Facts,
): Promise<SqlStatement> {
  return nativeNotificationStatement(
    db,
    actor,
    {
      companyId: facts.value2,
      userId: facts.value3,
      recordType: 'retirement_clearance',
      recordId: facts.value4,
      recordVersion: facts.value5,
      message: facts.value6,
    },
    facts.value1,
    facts.value7,
  );
}
