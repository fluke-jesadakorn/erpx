import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { CapabilityPurpose } from "@erpx/server-commercial/public/rules/packages";
import { record, type Snapshot } from "@erpx/server-pms/domain/pms";
import type { RoomSelection } from "@erpx/feature-pms/lib/pms-booking-domain";

export type PmsPropertyPurchase = {
  propertyId: string;
  purpose: CapabilityPurpose;
  readOnly?: boolean;
  hideUnavailable?: boolean;
  receipt?: { kind: "reservation" | "change"; id: string };
  actor?: WorkContext;
  permission?: "pms.read" | "pms.reservations" | "pms.approve" | "finance.approve";
};
/** Resolve a token/actor to its original owner only when the development switch
 * is enabled. The application repeats its normal authorization in the unit. */
export type PmsPropertyPurchaseRequest = PmsPropertyPurchase | (() => Promise<PmsPropertyPurchase | undefined>);

/** Cancelling or reducing the same reserved service settles existing work.
 * Changing rate/type or adding dates/guests creates new service capacity. */
export function bookingRevisionPurchasePurpose(snapshot: Snapshot, kind: "change" | "cancel", items: readonly { stayId: string; room?: RoomSelection; oldTotal?: number; newTotal?: number; dueNow?: number }[]): CapabilityPurpose {
  if (kind === "cancel") return "settle-existing";
  const reduced = items.every(item => {
    const old = record(snapshot, "stay", item.stayId).data, room = item.room;
    if (!room || room.quantity !== 1 || room.typeId !== old.typeId || room.rateId !== old.rateId || room.arrival < old.arrival || room.departure > old.departure || room.adults > old.adults || old.roomTypePeriods?.length || Number(item.dueNow ?? 0) > 0 || item.newTotal !== undefined && item.oldTotal !== undefined && item.newTotal > item.oldTotal) return false;
    const children = [...(old.childAges ?? [])];
    return room.childAges.every(age => { const index = children.indexOf(age); if (index < 0) return false; children.splice(index, 1); return true; });
  });
  return reduced ? "settle-existing" : "new-work";
}
