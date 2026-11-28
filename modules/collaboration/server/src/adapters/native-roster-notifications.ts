import 'server-only';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned roster notification statements (system context; no actor). */
export function rosterNotifications(database: SqlDatabase) {
  return {
    async applyEscalateRosterCoverage(
      fact0: string, fact1: string, fact2: string, fact3: string, fact4: string,
      fact5: string, fact6: string, fact7: number, fact8: string, fact9: number,
      fact10: number, fact11: string | null,
    ): Promise<any> {
      const stmt = await nativeSystemNotificationStatement(
        database, fact1,
        {
          userId: '',
          recordType: 'roster_coverage',
          recordId: fact5,
          recordVersion: 1,
          message: fact6,
          kind: 'roster_coverage',
          node_id: fact0,
          scope_node_id: fact11 ?? undefined,
        },
        fact3, fact7,
      );
      return stmt.run();
    },
    async applyNotice(
      fact0: string, fact1: string, fact2: string, fact3: 'roster_week' | 'roster_request',
      fact4: string, fact5: number, fact6: string, fact7: number,
    ): Promise<any> {
      const stmt = await nativeSystemNotificationStatement(
        database, fact1,
        { userId: fact2, recordType: fact3, recordId: fact4, recordVersion: fact5, message: fact6, kind: 'explicit' },
        fact0, fact7,
      );
      return stmt.run();
    },
    async applyNotifyManagers(
      fact0: string, fact1: string, fact2: string, fact3: string, fact4: number,
      fact5: 'A roster change needs your review.', fact6: number,
    ): Promise<any> {
      const stmt = await nativeSystemNotificationStatement(
        database, fact1,
        { userId: fact2, recordType: 'roster_request', recordId: fact3, recordVersion: fact4, message: fact5, kind: 'explicit' },
        fact0, fact6,
      );
      return stmt.run();
    },
  };
}
