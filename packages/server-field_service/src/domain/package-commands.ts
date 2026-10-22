import type { CapabilityPurpose } from "@erpx/server-commercial/public/packages";
type Scope = { capability: string; purpose: CapabilityPurpose };
const operations: Record<string, Scope> = {
  "field.agreement.create": { capability: "service.agreements", purpose: "new-work" },
  "field.agreement.activate": { capability: "service.agreements", purpose: "new-work" },
  "field.agreement.reject": { capability: "service.agreements", purpose: "settle-existing" },
  "field.agreement.retire": { capability: "service.agreements", purpose: "settle-existing" },
  "field.order.request": { capability: "service.orders", purpose: "new-work" },
  "field.order.create": { capability: "service.orders", purpose: "new-work" },
  "field.order.schedule": { capability: "service.dispatch", purpose: "settle-existing" },
  "field.order.dispatch": { capability: "service.dispatch", purpose: "settle-existing" },
  "field.order.start": { capability: "service.visits", purpose: "settle-existing" },
  "field.order.exception": { capability: "service.visits", purpose: "settle-existing" },
  "field.order.resume": { capability: "service.visits", purpose: "settle-existing" },
  "field.order.complete": { capability: "service.visits", purpose: "settle-existing" },
  "field.order.cancel": { capability: "service.orders", purpose: "settle-existing" },
  "field.order.release-billing": { capability: "service.billing", purpose: "settle-existing" },
  "field.order.reserve-part": { capability: "service.orders", purpose: "settle-existing" },
  "field.order.issue-part": { capability: "service.orders", purpose: "settle-existing" },
};
/** A new operation never inherits settlement or an optional add-on implicitly. */
export function serviceCommandPurchases(operation: string, usesAsset = false): Scope[] {
  if (!Object.hasOwn(operations, operation)) throw new Error(`SERVICE_PACKAGE_COMMAND_REQUIRED:${operation}`);
  const scope = operations[operation];
  return [{ ...scope }, ...(["field.order.request", "field.order.create"].includes(operation) ? [{capability:"support.source-ticket",purpose:scope.purpose}] : []),
    ...(operation === "field.order.release-billing" ? [{capability:"finance.service-invoice",purpose:scope.purpose}] : []), ...(usesAsset || operation === "field.order.reserve-part" || operation === "field.order.issue-part"
    ? [{ capability: "service.parts-assets", purpose: scope.purpose }] : [])];
}
