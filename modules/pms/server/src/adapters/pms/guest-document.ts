import "server-only";
import type { TransactionalSqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { authorizedBooking } from "@erpx/pms-server/public/pms-booking";
import { createGuestBookingDocument } from "@erpx/pms-server/application/guest-document";
import { nativePmsReadFirst, PMS_RD_GUEST_BOOKING_DOCUMENT } from "../native-pms-read";
export function guestBookingDocument(db: TransactionalSqlDatabase, token: string, raw: unknown, now = Date.now()) {
    return createGuestBookingDocument({ clock: { now: () => Date.now() }, error: (status, code, message) => new ApiError(status, code, message), repository: { transaction: work => db.transaction(tx => work({ authorized: (token, now) => authorizedBooking(tx, token, now), document: (id, propertyId, reservationId) => nativePmsReadFirst<{
                    kind: string;
                    snapshot_json: string;
                    created_at: number;
                }>(PMS_RD_GUEST_BOOKING_DOCUMENT, [id, propertyId, reservationId], tx) })) } })(token, raw, now);
}
