import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { WorkReportCandidate } from '@erpx/server-work_projection/public/work-reports';
import { readNativeWorkReportCandidates } from './native-lane-35/client';

/**
 * Lane 35 cutover. The owner query, kind predicate, submitted-time calculation
 * and response shape now live in the Rust axum/sqlx route. This adapter is a
 * thin transport only and fails closed when the native reader is unavailable.
 */
export async function workReportRecordsCandidates(
  _db: SqlDatabase,
  companyId: string,
): Promise<WorkReportCandidate[]> {
  return (await readNativeWorkReportCandidates(companyId)) as unknown as WorkReportCandidate[];
}
