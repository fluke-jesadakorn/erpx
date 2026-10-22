import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";

/** Closed POS source projection: preserve the original Company/Branch fence and first sale order. */
export function findPosMovementSale(db: SqlDatabase, movementId: string, companyId: string, branchId: string | null) {
  return db.prepare("SELECT l.sale_id FROM pos_stock_moves m JOIN pos_lines l ON l.fulfillment_id=m.fulfillment_id JOIN pos_sales s ON s.id=l.sale_id WHERE m.id=? AND s.company_id=? AND (?::text IS NULL OR s.branch_id=?) ORDER BY s.created_at LIMIT 1")
    .bind(movementId, companyId, branchId, branchId).first<{ sale_id: string }>();
}

export async function posMovementSaleForPresentation(actor: Pick<WorkContext, "holdingId" | "companyId" | "branchId">, movementId: string) {
  const { write } = await holdingResources(actor.holdingId);
  return findPosMovementSale(write, movementId, actor.companyId, actor.branchId);
}
