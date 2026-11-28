import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance due-draft statements. */
export function financeDueDrafts(db: SqlDatabase, companyId: string) {
  const call = (kind: string, key: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, key, 0);
  return {
    schedule: (f: {
      id: string; companyId: string; holdingId: string; kind: string; sourceId: string; runOn: string;
      sourceVersion: number; inputJson: string; inputHash: string; userId: string; assignmentId: string;
      assignmentVersion: number; role: string; reason: string; now: number;
    }) => call('finance_due_draft_schedule', f.id,
      [f.id, f.companyId, f.holdingId, f.kind, f.sourceId, f.runOn, f.sourceVersion, f.inputJson, f.inputHash, f.userId, f.assignmentId, f.assignmentVersion, f.role, f.reason, f.now, f.now]),
    decide: (f: { status: string; now: number; id: string }) =>
      call('finance_due_draft_decide', '', [f.status, f.now, f.id]),
  };
}

import { nativeHoldingNotificationStatement } from './native-system-notification';

/** Native-owned finance due-draft worker statements (holding-keyed, company-less). */
export function financeDueDraftsWorker(db: SqlDatabase, holdingId: string) {
  const call = (kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeHoldingNotificationStatement(db, holdingId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return {
    complete: (f: { result: unknown; time: number; id: string | number | null }) => call('finance_due_draft_complete', [JSON.stringify(f.result), f.time, f.id]),
    block: (f: { code: string; message: string; time: number; id: string | number | null }) => call('finance_due_draft_block', [f.code, f.message, f.time + 3600000, f.time, f.id]),
  };
}
