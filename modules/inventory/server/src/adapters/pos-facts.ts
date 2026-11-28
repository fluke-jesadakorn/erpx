import 'server-only';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { readStockAvailability, type Lane18Actor } from './native-lane-18/client';

/** Owner-native reads for the POS settings stock snapshot. The retired SQL
 * joined Commerce warehouse/product projections and Inventory reservation and
 * Quality-held facts; the axum lane-18 reader now owns that statement and its
 * Company/branch scope. This adapter only transports bounded scope values.
 *
 * A caller with a WorkContext should pass `actor` so the target Holding is
 * selected from `(holdingId, holdingSlug)`; a legacy fact caller without one
 * is pinned by the exact incoming Host. There is no first-Holding default. */
export function PosReadInventoryFacts(db: SqlDatabase, actor?: Lane18Actor) {
  void db;
  return {
    readInventoryActiveReservations1: (facts: { company: string; branch: string | null; branch2: string | null }) =>
      readStockAvailability(actor, facts.company, facts.branch),
  };
}
