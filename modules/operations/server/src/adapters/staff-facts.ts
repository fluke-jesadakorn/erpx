import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { nativeLane28StaffLevels, type Lane28WorkContext } from './native-lane-28/client';

/**
 * Native owner read: active staff-level ranks for legacy route eligibility.
 *
 * The retired inline ranks projection is now executed by the Rust axum owner
 * with sqlx (lane 28).
 * This adapter is a strict transport: it emits no SQL and fails closed when
 * the native owner is unavailable. The caller's unit remains in the signature
 * for transport compatibility and is intentionally unused.
 */
export async function activeStaffLevelRanks(
  _db: SqlDatabase,
  companyId: string,
  actor?: Lane28WorkContext,
) {
  const facts = await nativeLane28StaffLevels(companyId, actor);
  return facts.levels.map(({ id, rank }) => ({ id, rank }));
}
