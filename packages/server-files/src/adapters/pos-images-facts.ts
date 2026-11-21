import "server-only";
import {one,run} from '@erpx/server-runtime/public/runtime/sql';
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function posImagesFilesFacts(db: SqlDatabase) {
    return {
        readAttachments1: (facts: {
            sourceId: string;
            companyId: string;
        }) => one(db, "SELECT object_key,preview_object_key,preview_checksum_sha256 FROM attachments WHERE id=? AND company_id=? AND source_state='available'", facts.sourceId, facts.companyId),
    };
}
