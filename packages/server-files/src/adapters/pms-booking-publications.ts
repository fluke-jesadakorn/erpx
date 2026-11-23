import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { nativeFilesReadAll } from "./native-files-command";

/**
 * PMS published-booking publication facts. The Files-owned read (approved
 * publications by id) is owned by the native Files read kind
 * `RD_PMS_BOOKING_PUBLICATIONS`; this module emits no Files SQL and fails
 * closed without the native transport. The PMS-owned publication-id selector
 * stays with the caller and its rows are passed as one JSON-array bind so the
 * owner statement keeps static arity; the caller's `.results` shape is kept.
 */
export function pmsBookingPublications(db: SqlDatabase, referenceSelector: string) {
    return {
        approved: async () => {
            const ids = (await db.prepare(referenceSelector).all<{ publicationId: string }>()).results
                .map(row => row.publicationId)
                .filter((id): id is string => !!id);
            const result = await nativeFilesReadAll<{ id: string }>(
                'RD_PMS_BOOKING_PUBLICATIONS',
                [JSON.stringify(ids)],
                db,
            );
            return result.results;
        },
    };
}
