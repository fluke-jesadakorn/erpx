import type {HoldingWorkspaceScope} from "@/lib/work_projection/workspace";
export type FrontAppKind = "pos" | "pms";
export type FrontLocation = { key: string; label: string; path: string };
const frontPosOperations = new Set([
  "sale.create", "sale.add", "sale.remove", "sale.hold", "sale.resume",
  "sale.update_line", "sale.discount", "sale.customer", "sale.move_table", "sale.split", "sale.merge", "customer.create",
  "sale.cancel", "sale.complete", "sale.charge_to_room", "payment.create", "payment.retry",
  "payment.cancel", "payment.cancel_transfer", "refund.request", "room_charge_reversal.request", "room_charge_reversal.approve_pos", "room_charge_reversal.reject_pos",
  "kitchen.send", "kitchen.start", "kitchen.ready", "kitchen.deliver",
  "kitchen.cancel", "shift.open", "shift.close", "cash.create",
]);
const frontPmsOperations = new Set([
  "reservation.create", "reservation.cancel", "reservation.no_show",
  "stay.cancel", "stay.no_show", "stay.assign", "stay.move", "stay.upgrade", "stay.guest",
  "stay.route_guest_account", "stay.check_in", "stay.preview_checkout",
  "stay.check_out", "stay.change_dates", "charge.add", "payment.receive",
  "payment.deposit", "payment.apply_deposit", "payment.request_transfer",
  "payment.cancel_transfer", "refund.request", "refund.pay", "charge.correction.request", "guest_account.close", "shift.open",
  "cash.move", "shift.close", "task.create", "task.assign", "task.start",
  "task.complete", "task.inspect", "room.dnd", "room.observe",
  "maintenance.open", "maintenance.close",
  "night.start", "night.resume",
]);
export const frontPmsSections = new Set([
  "dashboard", "reservations", "front-desk", "rooms", "housekeeping",
  "guest-accounts", "cashier", "night-audit",
]);
export type FrontCompanyBrand = {
  holdingId: string;
  holdingSlug: string;
  companyId: string;
  companySlug: string;
  companyName: string;
  shortName: string;
  accentColor: string;
  logoAttachmentId: string | null;
  logoPublicationId: string | null;
};
export type FrontCompanyApp = FrontCompanyBrand & {
  userId: string;
  locations: FrontLocation[];
};
export function frontOperationAllowed(app: FrontAppKind, operation: string) {
  return (app === "pos" ? frontPosOperations : frontPmsOperations).has(operation);
}
export function localPathFromDestination(destination: string | undefined, holdingSlug: string) {
  if (!destination) return null;
  const prefix = "/holdings/" + encodeURIComponent(holdingSlug);
  if (!destination.startsWith(prefix)) return null;
  return destination.slice(prefix.length).replace(/\/home(?:\/.*)?$/, "").replace(/^\/+|\/+$/g, "") || "holding";
}
export function frontCookieName(app: FrontAppKind, entry: Pick<FrontCompanyApp, "companyId" | "userId">) {
  return ("erpx_front_" + app + "_" + entry.companyId + "_" + entry.userId).replace(/[^A-Za-z0-9_-]/g, "_");
}
export function frontLocationScope(holdingSlug: string, locationPath: string): HoldingWorkspaceScope | null {
  const clean = locationPath.replace(/^\/+|\/+$/g, "");
  if (clean === "holding") return { kind: "holding", holdingSlug };
  const parts = clean.split("/");
  if (parts[0] === "branches" && parts[1] && parts.length === 2)
    return { kind: "branch", holdingSlug, branchSlug: parts[1] };
  if (parts[0] === "departments" && parts[1] && parts.length === 2)
    return { kind: "department", holdingSlug, departmentSlug: parts[1] };
  if (parts[0] === "branches" && parts[1] && parts[2] === "departments" && parts[3] && parts.length === 4)
    return { kind: "department", holdingSlug, branchSlug: parts[1], departmentSlug: parts[3] };
  return null;
}
export function frontRouteForLocation(base: string, app: FrontAppKind, locationPath: string) {
  const location = locationPath === "holding" ? "/holding" : "/" + locationPath.replace(/^\/+/, "");
  return base + "/" + app + location + "/";
}
