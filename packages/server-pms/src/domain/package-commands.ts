import type { CapabilityPurpose } from "@erpx/server-commercial/public/rules/packages";
export type PmsCommandPurchase = { capability: string; purpose: CapabilityPurpose };

export function pmsCommandPurchases(operation: string, input: unknown): PmsCommandPurchase[] {
  const data = input && typeof input === "object" ? input as Record<string, unknown> : {};
  const settlement = operation.startsWith("payment.") || operation.startsWith("refund.") || operation.startsWith("credit.")
    || operation.startsWith("cash.") || operation.startsWith("night.") || operation.startsWith("charge.correction.")
    || ["reservation.cancel", "reservation.no_show", "block.release", "stay.cancel", "stay.no_show", "stay.assign", "stay.guest", "stay.route_guest_account", "stay.check_in", "stay.check_out", "stay.preview_checkout", "charge.reverse", "guest_account.transfer", "guest_account.merge", "guest_account.close", "ota.settle", "shift.close", "shift.review", "task.assign", "task.start", "task.complete", "task.inspect", "maintenance.close", "lost.release", "booking.retry_email", "booking.cancellation_fee", "linen.return", "linen.wash", "linen.clean", "linen.loss"].includes(operation)
    || operation === "guest.save" && typeof data.id === "string" && !!data.id
    || operation === "guest.service_request" && typeof data.requestId === "string" && !!data.requestId && ["completed", "cancelled"].includes(String(data.state));
  // The owner retries only an existing failed mail with its frozen payload.
  const purpose = operation === "booking.retry_email" ? "history" : settlement ? "settle-existing" : "new-work";
  const capability = operation.startsWith("night.") ? "pms.night-audit"
    : operation.startsWith("reservation.") || operation.startsWith("booking.") ? "pms.reservations"
      : operation.startsWith("room.") || operation.startsWith("room_type.") || operation.startsWith("rate.") || operation.startsWith("block.") ? "pms.rooms"
        : operation.startsWith("task.") || operation.startsWith("maintenance.") || operation.startsWith("lost.") || operation.startsWith("item.") || operation.startsWith("minibar.") || operation.startsWith("linen.") || operation.startsWith("supply.") ? "pms.housekeeping"
          : operation.startsWith("guest_account.") || operation.startsWith("charge.") || operation.startsWith("payment.") || operation.startsWith("refund.") || operation.startsWith("credit.") || operation.startsWith("shift.") || operation.startsWith("cash.") || operation.startsWith("ota.") ? "pms.guest-accounts" : "pms.front-desk";
  const scopes: PmsCommandPurchase[] = [{ capability, purpose }];
  if (operation.startsWith("minibar.") || operation === "item.save" && data.kind === "minibar") scopes.push({ capability: "pms.minibar", purpose });
  if (operation.startsWith("linen.") || operation === "item.save" && data.kind === "linen") scopes.push({ capability: "pms.linen", purpose });
  if (operation.startsWith("minibar.") || operation.startsWith("linen.") || operation.startsWith("supply.")) scopes.push({ capability: "inventory.stock-core", purpose });
  if (operation.startsWith("charge.") || operation.startsWith("payment.") || operation.startsWith("refund.") || operation.startsWith("credit.") || operation.startsWith("night.")
    || operation.startsWith("ota.") || operation.startsWith("minibar.") || ["stay.check_in", "stay.check_out", "guest_account.transfer", "guest_account.merge", "guest_account.close"].includes(operation)) scopes.push({ capability: "finance.hotel-posting", purpose });
  return scopes;
}
