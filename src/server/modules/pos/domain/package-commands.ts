import type { CapabilityPurpose } from "../../commercial/public/rules/packages";

export type PosPurchaseScope = { capability: string; purpose: CapabilityPurpose };
/** Provider work settles an identified owner record; it never borrows the
 * originating cashier's now possibly revoked assignment. */
export type PosProviderPurchase = { companyId: string; operation: "payment.settle" | "refund.settle" | "sale.complete"; recordId: string };
/** Existing receipts, refunds, kitchen fulfilment and day/shift closing remain
 * settlement work. Increasing a cart or starting a shift requires active work. */
export function posCommandPurchases(operation: string, restaurant = false): PosPurchaseScope[] {
  const settlement = operation.startsWith("refund.") || operation.startsWith("room_charge_reversal.")
    || operation.startsWith("payment.") || operation.startsWith("kitchen.") || operation.startsWith("day.")
    || operation.startsWith("settlement.") || operation.startsWith("exception.") || operation.startsWith("cash.")
    || ["shift.close", "shift.review", "sale.complete", "sale.cancel", "sale.remove", "sale.discount", "sale.approve_discount", "sale.charge_to_room", "sale.reverse_room_charge"].includes(operation);
  const purpose = settlement ? "settle-existing" : "new-work";
  const capability = operation.startsWith("refund.") ? "pos.refunds"
    : operation.startsWith("payment.") || operation.startsWith("connection.") || operation.startsWith("settlement.") || operation.startsWith("exception.") ? "pos.payments"
      : operation.startsWith("shift.") || operation.startsWith("cash.") || operation.startsWith("day.") ? "pos.shifts" : "pos.sales";
  const scopes: PosPurchaseScope[] = [{ capability, purpose }];
  if (restaurant || operation.startsWith("table.")) scopes.push({ capability: "pos.restaurant", purpose });
  if (operation.startsWith("kitchen.")) scopes.push({ capability: "pos.kitchen", purpose });
  if (operation === "sale.charge_to_room" || operation === "sale.reverse_room_charge" || operation.startsWith("room_charge_reversal.")) {
    // The first room charge creates a new Hotel obligation. Reversing a
    // recorded charge remains settlement; committed retries recover history.
    const hotelPurpose = operation === "sale.charge_to_room" ? "new-work" : purpose;
    scopes.push({ capability: "pos.hotel-charge", purpose: hotelPurpose }, { capability: "pms.guest-accounts", purpose: hotelPurpose }, { capability: "finance.hotel-posting", purpose: hotelPurpose });
  }
  if (operation.startsWith("sale.") || operation.startsWith("refund.") || operation.startsWith("kitchen.") || operation.startsWith("room_charge_reversal."))
    scopes.push({ capability: "inventory.stock-core", purpose });
  if (operation.startsWith("payment.") || operation.startsWith("refund.") || operation.startsWith("day.") || operation.startsWith("settlement.")
    || ["sale.complete", "sale.charge_to_room", "sale.reverse_room_charge"].includes(operation) || operation.startsWith("room_charge_reversal."))
    scopes.push({ capability: "finance.pos-posting", purpose });
  return scopes;
}
