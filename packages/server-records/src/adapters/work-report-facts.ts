import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { readNativeWorkReportEvents } from './native-lane-35/client';

type WorkReportEventRow = {
  record_id: string;
  action: string;
  actor: string;
  at: number;
  reason: string;
};

type WorkReportRows<T> = { results: T[] };

/**
 * Lane 35 cutover. The owner query and the legacy `ORDER BY created_at,id` now
 * live in the Rust axum/sqlx route. This adapter is a thin transport only and
 * fails closed when the native reader is unavailable.
 */
export function workReportRecordsFacts(_db: SqlDatabase) {
  return {
    events: async (companyId: string, ids: string): Promise<WorkReportRows<WorkReportEventRow>> => ({
      results: await readNativeWorkReportEvents(companyId, ids),
    }),
  };
}
