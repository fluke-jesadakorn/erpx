import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { attachmentSetPurposeSql } from "./read-tail-dedupe-sql";
export function pmsFilesFacts(db: SqlDatabase) {
    return {
        readAttachmentSets: (facts: {
            companyId: string;
            attachmentSetIds: readonly string[];
        }) => db.prepare(attachmentSetPurposeSql(facts.attachmentSetIds.map(() => "?").join(","))).bind(facts.companyId, ...facts.attachmentSetIds).all<{
            id: string;
            purpose_key: string;
            component_kind: string;
            component_ref: string | null;
        }>(),
        readAttachmentPublications: (facts: {
            publicationId: string;
            companyId: string;
            targetType: string;
            targetId: string;
        }) => db.prepare("SELECT id FROM attachment_publications WHERE id=? AND company_id=? AND target_type=? AND target_id=? AND state='approved'").bind(facts.publicationId, facts.companyId, facts.targetType, facts.targetId).first()
    };
}
