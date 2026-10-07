import "server-only";
import type { TransactionalSqlDatabase } from "../../../runtime/public/runtime/types";
import { ApiError } from "../../../runtime/public/http";
import { authorizedBooking } from "../../public/pms-booking";
import { createGuestBookingDocument } from "../../application/guest-document";
export function guestBookingDocument(db: TransactionalSqlDatabase, token: string, raw: unknown, now = Date.now()) {
    return createGuestBookingDocument({ clock: { now: () => Date.now() }, error: (status, code, message) => new ApiError(status, code, message), repository: { transaction: work => db.transaction(tx => work({ authorized: (token, now) => authorizedBooking(tx, token, now), document: (id, propertyId, reservationId) => tx.prepare(`SELECT d.kind,d.snapshot_json,d.created_at FROM pms_documents d JOIN pms_records f ON f.id=d.folio_id AND f.property_id=d.property_id AND f.kind='folio'
    WHERE d.id=? AND d.property_id=? AND f.data_json::jsonb->>'reservationId'=? AND d.kind IN ('invoice','receipt','refund','credit_note')`).bind(id, propertyId, reservationId).first<{
                    kind: string;
                    snapshot_json: string;
                    created_at: number;
                }>() })) } })(token, raw, now);
}
