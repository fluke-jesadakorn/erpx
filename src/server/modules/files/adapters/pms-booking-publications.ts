import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
export function pmsBookingPublications(db: SqlDatabase, referenceSelector: string) { return { approved: async () => (await db.prepare(`SELECT id FROM attachment_publications WHERE state='approved' AND id IN (${referenceSelector})`).all<{
        id: string;
    }>()).results }; }
