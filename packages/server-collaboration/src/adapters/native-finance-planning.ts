import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance planning statements. */
export function financePlanning(db: SqlDatabase, companyId: string) {
  const call = (kind: string, key: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, key, 0);
  return {
    createEdition: (f: {
      id: string; companyId: string; chartId: string; kind: string; name: string;
      parentId: string | null; edition: number; contentJson: string; hash: string; userId: string; now: number;
    }) => call('finance_plan_edition_create', f.id,
      [f.id, f.companyId, f.chartId, f.kind, f.name, f.parentId, f.edition, f.contentJson, f.hash, f.userId, f.now]),
    reviewEdition: (f: { status: string; userId: string; reason: string; now: number; id: string }) =>
      call('finance_plan_edition_review', '', [f.status, f.userId, f.reason, f.now, f.id]),
    createSnapshot: (f: {
      snapshotId: string; companyId: string; definitionId: string; startDate: string; endDate: string;
      resultJson: string; sourceHash: string; userId: string; now: number; csv: string; exportHash: string;
    }) => call('finance_report_snapshot', f.snapshotId,
      [f.snapshotId, f.companyId, f.definitionId, f.startDate, f.endDate, f.resultJson, f.sourceHash, f.userId, f.now, f.csv, f.exportHash]),
  };
}
