export type MaterialLotAllocation = { productId: string; lotId: string; quantity: string; balanceVersion: number };
export type OutputLotAllocation = { quantity: string; lotCode?: string; serialNumber?: string; expiryDate?: string };
export type LotViolation = { status: number; code: string; message: string };
export function materialAllocationViolation(trackingMode: string, requiredMilli: number, allocations: MaterialLotAllocation[], quantityMilli: (quantity: string) => number): LotViolation | null {
  if (trackingMode === "none") return allocations.length ? { status: 422, code: "LOT_ALLOCATION_UNEXPECTED", message: "This manufacturing material does not use lot tracking." } : null;
  if (!allocations.length || allocations.reduce((sum, allocation) => sum + quantityMilli(allocation.quantity), 0) !== requiredMilli)
    return { status: 422, code: "MANUFACTURING_LOT_ALLOCATION_REQUIRED", message: "Allocate the entire issued material quantity to its available lots or serial numbers." };
  if (new Set(allocations.map(allocation => allocation.lotId)).size !== allocations.length)
    return { status: 422, code: "LOT_ALLOCATION_DUPLICATE", message: "Use each material lot or serial once in this run." };
  return null;
}
export function materialLotViolation(lot: Record<string, unknown> | null, facts: { balanceVersion: number; quantity: number; trackingMode: string; entryDate: string }): LotViolation | null {
  if (!lot || lot.status !== "active" || lot.held || Number(lot.balance_version) !== facts.balanceVersion || Number(lot.quantity_milli) < facts.quantity || (lot.expiry_date && String(lot.expiry_date) < facts.entryDate))
    return { status: 409, code: "MANUFACTURING_LOT_UNAVAILABLE", message: "A selected material lot is changed, expired, quarantined or unavailable." };
  if (facts.trackingMode === "serial" && facts.quantity !== 1000)
    return { status: 422, code: "SERIAL_QUANTITY_INVALID", message: "Issue each material serial as exactly one unit." };
  return null;
}
export function outputAllocationViolation(trackingMode: string, outputMilli: number, allocations: OutputLotAllocation[], quantityMilli: (quantity: string) => number): LotViolation | null {
  if (trackingMode === "none") return allocations.length ? { status: 422, code: "LOT_ALLOCATION_UNEXPECTED", message: "This finished product does not use lot or serial tracking." } : null;
  if (!allocations.length || allocations.reduce((sum, allocation) => sum + quantityMilli(allocation.quantity), 0) !== outputMilli)
    return { status: 422, code: "MANUFACTURING_OUTPUT_LOTS_REQUIRED", message: "Allocate the accepted output quantity to new production lots or serial numbers." };
  const identities = allocations.map(allocation => trackingMode === "serial" ? allocation.serialNumber : allocation.lotCode);
  if (identities.some(identity => !identity) || new Set(identities).size !== identities.length)
    return { status: 422, code: "MANUFACTURING_OUTPUT_IDENTITY", message: "Use a unique lot code or serial number for each output allocation." };
  return null;
}
