import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import {
  nativeManufacturingLane28PostingReferences,
  type Lane28WorkContext,
} from './native-lane-28/client';

/** Actual original posting IDs, including an absence witness when a previously
 *  recorded posting no longer satisfies its immutable owner/receipt lineage.
 *
 *  The retired inline statement + scoped EXISTS witness is now owned and executed
 *  by the Rust axum owner with sqlx (lane 28). This adapter is a strict
 *  transport: it emits no SQL and fails closed when the native owner is
 *  unavailable. The caller's unit stays in the signature for compatibility. */
export async function manufacturingExistingPostingEconomicReferences(
  _database: SqlDatabase,
  companyId: string,
  actor?: Lane28WorkContext,
) {
  return nativeManufacturingLane28PostingReferences(companyId, actor);
}
