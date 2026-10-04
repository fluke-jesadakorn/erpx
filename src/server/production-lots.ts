import "server-only";
import type { WorkContext } from "./authorization";
import type { Row } from "./commerce";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
import { assertion } from "./commands";
import { ApiError } from "./http";
import { quantityMilli, scaledAmount } from "@/lib/commerce-math";

export type ManufacturingMaterialAllocation = { productId: string; lotId: string; quantity: string; balanceVersion: number };
export type ManufacturingOutputAllocation = { quantity: string; lotCode?: string; serialNumber?: string; expiryDate?: string };
export async function prepareManufacturingMaterialLots(db: SqlDatabase, a: WorkContext, input: {
  key: string; orderId: string; orderVersion: number; warehouseId: string; productId: string; trackingMode: string;
  requiredMilli: number; entryDate: string; allocations: ManufacturingMaterialAllocation[];
}) {
  const allocations = input.allocations.filter(x => x.productId === input.productId);
  if (input.trackingMode === "none") {
    if (allocations.length) throw new ApiError(422, "LOT_ALLOCATION_UNEXPECTED", "This manufacturing material does not use lot tracking.");
    return null;
  }
  if (!allocations.length || allocations.reduce((n, x) => n + quantityMilli(x.quantity), 0) !== input.requiredMilli)
    throw new ApiError(422, "MANUFACTURING_LOT_ALLOCATION_REQUIRED", "Allocate the entire issued material quantity to its available lots or serial numbers.");
  if (new Set(allocations.map(x => x.lotId)).size !== allocations.length)
    throw new ApiError(422, "LOT_ALLOCATION_DUPLICATE", "Use each material lot or serial once in this run.");
  let valueMinor = 0;
  const steps: SqlStatement[] = [];
  for (const [n, allocation] of allocations.entries()) {
    const quantity = quantityMilli(allocation.quantity), lot = await db.prepare(`SELECT l.*,b.quantity_milli,b.value_minor,b.version balance_version,
      EXISTS(SELECT 1 FROM inventory_quality_holds h WHERE h.warehouse_id=b.warehouse_id AND h.lot_id=l.id AND h.status NOT IN ('released','cancelled','disposed')) held
      FROM inventory_lots l JOIN inventory_lot_balances b ON b.lot_id=l.id WHERE l.id=? AND l.company_id=? AND l.product_id=? AND b.warehouse_id=?`)
      .bind(allocation.lotId, a.companyId, input.productId, input.warehouseId).first<Row>();
    if (!lot || lot.status !== "active" || lot.held || Number(lot.balance_version) !== allocation.balanceVersion || Number(lot.quantity_milli) < quantity || (lot.expiry_date && String(lot.expiry_date) < input.entryDate))
      throw new ApiError(409, "MANUFACTURING_LOT_UNAVAILABLE", "A selected material lot is changed, expired, quarantined or unavailable.");
    if (input.trackingMode === "serial" && quantity !== 1000)
      throw new ApiError(422, "SERIAL_QUANTITY_INVALID", "Issue each material serial as exactly one unit.");
    const value = quantity === Number(lot.quantity_milli) ? Number(lot.value_minor) : scaledAmount(Number(lot.value_minor), quantity, Number(lot.quantity_milli)), now = Date.now();
    valueMinor += value;
    steps.push(db.prepare("SELECT lot_id FROM inventory_lot_balances WHERE lot_id=? AND warehouse_id=? FOR UPDATE").bind(allocation.lotId, input.warehouseId),
      assertion(db, input.key, `material-lot-${input.productId}-${n}`, `EXISTS(SELECT 1 FROM inventory_lot_balances b JOIN inventory_lots l ON l.id=b.lot_id
        WHERE b.lot_id=? AND b.warehouse_id=? AND b.version=? AND b.quantity_milli>=? AND b.value_minor>=? AND l.status='active'
        AND (l.expiry_date IS NULL OR l.expiry_date>=?) AND NOT EXISTS(SELECT 1 FROM inventory_quality_holds h WHERE h.warehouse_id=b.warehouse_id
          AND (h.lot_id=l.id OR (h.lot_id IS NULL AND h.product_id=l.product_id)) AND h.status NOT IN ('released','cancelled','disposed')))`,
        [allocation.lotId, input.warehouseId, allocation.balanceVersion, quantity, value, input.entryDate]),
      db.prepare("UPDATE inventory_lot_balances SET quantity_milli=quantity_milli-?,value_minor=value_minor-?,version=version+1 WHERE lot_id=? AND warehouse_id=?")
        .bind(quantity, value, allocation.lotId, input.warehouseId),
      db.prepare("UPDATE inventory_lots SET status='depleted' WHERE id=? AND NOT EXISTS(SELECT 1 FROM inventory_lot_balances WHERE lot_id=? AND quantity_milli>0)").bind(allocation.lotId, allocation.lotId),
      db.prepare("INSERT INTO manufacturing_material_lot_issues VALUES(?,?,?,?,?,?,?,?,?)").bind(`mfg-input-lot-${input.key}-${input.productId}-${n}`, input.orderId, input.orderVersion, input.productId, allocation.lotId, quantity, value, a.userId, now),
      db.prepare("INSERT INTO inventory_lot_moves(id,lot_id,warehouse_id,source_type,source_id,quantity_milli,value_minor,movement_date,created_by,created_at) VALUES(?,?,?,'manufacturing_issue',?,?,?,?,?,?)")
        .bind(`mfg-input-move-${input.key}-${input.productId}-${n}`, allocation.lotId, input.warehouseId, `${input.orderId}:v${input.orderVersion}:${input.productId}`, -quantity, -value, input.entryDate, a.userId, now),
    );

  }
  return { steps, valueMinor };
}

export async function prepareManufacturingOutputLots(db: SqlDatabase, a: WorkContext, input: {
  key: string; orderId: string; orderVersion: number; warehouseId: string; productId: string; trackingMode: string;
  outputMilli: number; totalMinor: number; entryDate: string; allocations: ManufacturingOutputAllocation[];
}) {
  if (input.trackingMode === "none") {
    if (input.allocations.length) throw new ApiError(422, "LOT_ALLOCATION_UNEXPECTED", "This finished product does not use lot or serial tracking.");
    return [];
  }
  if (!input.allocations.length || input.allocations.reduce((n, x) => n + quantityMilli(x.quantity), 0) !== input.outputMilli)
    throw new ApiError(422, "MANUFACTURING_OUTPUT_LOTS_REQUIRED", "Allocate the accepted output quantity to new production lots or serial numbers.");
  const identities = input.allocations.map(x => input.trackingMode === "serial" ? x.serialNumber : x.lotCode);
  if (identities.some(x => !x) || new Set(identities).size !== identities.length)
    throw new ApiError(422, "MANUFACTURING_OUTPUT_IDENTITY", "Use a unique lot code or serial number for each output allocation.");
  const issues = (await db.prepare("SELECT * FROM manufacturing_material_lot_issues WHERE order_id=? ORDER BY id").bind(input.orderId).all<Row>()).results;
  const steps: SqlStatement[] = [], priorInput = new Map<string, number>();
  let cumulativeQty = 0, cumulativeValue = 0;
  for (const [n, allocation] of input.allocations.entries()) {
    const quantity = quantityMilli(allocation.quantity);
    if (input.trackingMode === "serial" && quantity !== 1000) throw new ApiError(422, "SERIAL_QUANTITY_INVALID", "Each output serial must identify one unit.");
    if (allocation.expiryDate && allocation.expiryDate < input.entryDate) throw new ApiError(422, "MANUFACTURING_OUTPUT_EXPIRY", "New output cannot already be expired.");
    cumulativeQty += quantity;
    const cumulative = n === input.allocations.length - 1 ? input.totalMinor : scaledAmount(input.totalMinor, cumulativeQty, input.outputMilli),
      value = cumulative - cumulativeValue, lotId = `mfg-output-lot-${input.key}-${n}`, outputId = `mfg-output-${input.key}-${n}`, now = Date.now();
    cumulativeValue = cumulative;
    steps.push(db.prepare("INSERT INTO inventory_lots(id,company_id,product_id,tracking_mode,lot_code,serial_number,manufactured_date,expiry_date,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)")
      .bind(lotId, a.companyId, input.productId, input.trackingMode, allocation.lotCode ?? null, input.trackingMode === "serial" ? allocation.serialNumber! : null, input.entryDate, allocation.expiryDate ?? null, a.userId, now),
      db.prepare("INSERT INTO inventory_lot_balances(lot_id,warehouse_id,quantity_milli,value_minor) VALUES(?,?,?,?)").bind(lotId, input.warehouseId, quantity, value),
      db.prepare("INSERT INTO manufacturing_output_lots VALUES(?,?,?,?,?,?,?,?)").bind(outputId, input.orderId, input.orderVersion, lotId, quantity, value, a.userId, now),
      db.prepare("INSERT INTO inventory_lot_moves(id,lot_id,warehouse_id,source_type,source_id,quantity_milli,value_minor,movement_date,created_by,created_at) VALUES(?,?,?,'manufacturing_output',?,?,?,?,?,?)")
        .bind(`mfg-output-move-${input.key}-${n}`, lotId, input.warehouseId, `${input.orderId}:v${input.orderVersion}`, quantity, value, input.entryDate, a.userId, now),
    );
    for (const [issueIndex, issue] of issues.entries()) {
      const cumulativeInput = n === input.allocations.length - 1 ? Number(issue.quantity_milli) : scaledAmount(Number(issue.quantity_milli), cumulativeQty, input.outputMilli),
        inputQty = cumulativeInput - (priorInput.get(String(issue.id)) ?? 0);
      priorInput.set(String(issue.id), cumulativeInput);
      steps.push(db.prepare("INSERT INTO manufacturing_lot_genealogy VALUES(?,?,?,?,?,?,?)").bind(`mfg-lot-genealogy-${input.key}-${n}-${issueIndex}`, input.orderId, issue.id, outputId, inputQty, input.orderVersion, now));
    }
  }
  return steps;
}
