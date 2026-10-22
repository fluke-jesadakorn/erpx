import "server-only";
import { one, run } from "@erpx/server-pos/public/pos-core";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function posImagesFilesFacts(db: SqlDatabase) {
    return {
        readAttachments1: (facts: {
            sourceId: string;
            companyId: string;
        }) => one(db, "SELECT object_key,preview_object_key,preview_checksum_sha256 FROM attachments WHERE id=? AND company_id=? AND source_state='available'", facts.sourceId, facts.companyId),
        writeAttachmentEvents1: (facts: {
            fact0: string;
            companyId: string;
            attachment_source_id: string;
            attachment_link_id: string;
            attachment_set_id: string;
            userId: string;
            fact6: string;
            fact7: number;
        }) => run(db, "INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,action,details_json,created_at) VALUES(?,?,?,?,?,?,'content.read',?::jsonb,?)", facts.fact0, facts.companyId, facts.attachment_source_id, facts.attachment_link_id, facts.attachment_set_id, facts.userId, facts.fact6, facts.fact7)
    };
}
