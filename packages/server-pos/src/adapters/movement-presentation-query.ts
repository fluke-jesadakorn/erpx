import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { nativePosMovementSaleStatement } from "./native-pos-core-statement";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";

/** Closed POS source projection: preserve the original Company/Branch fence and first sale order.
 *  Served by the native POS owner statement registry (`pos_readposmovementsale_530`);
 *  no local SQL mirror or fallback reader remains. */
export function findPosMovementSale(db: SqlDatabase, movementId: string, companyId: string, branchId: string | null) {
  return nativePosMovementSaleStatement<{ sale_id: string }>(db, movementId, companyId, branchId);
}

export async function posMovementSaleForPresentation(actor: Pick<WorkContext, "holdingId" | "companyId" | "branchId">, movementId: string) {
  const { write } = await holdingResources(actor.holdingId);
  return findPosMovementSale(write, movementId, actor.companyId, actor.branchId);
}
