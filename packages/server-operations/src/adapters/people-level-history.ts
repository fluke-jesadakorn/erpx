import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { nativeLane28LevelHistory, type Lane28WorkContext } from './native-lane-28/client';

/** Native owner read: one engagement's staff-level history. The retired inline
 *  projection is executed by the Rust axum owner with sqlx (lane 28); this
 *  adapter is a strict transport that emits no SQL and fails closed. The
 *  caller's unit is retained for transport compatibility. Holding selection
 *  requires a WorkContext or an explicit company identity — there is no
 *  first-holding default. */
export function peopleLevelHistory(
  _db: SqlDatabase,
  engagementId: unknown,
  actor?: Lane28WorkContext,
  companyId?: string,
) {
  return nativeLane28LevelHistory(String(engagementId), actor, companyId);
}
