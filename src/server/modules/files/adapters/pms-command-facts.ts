import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
export function pmsFilesFacts(db: SqlDatabase) {
    return {
        readAttachmentSets: (facts: {
            companyId: string;
            attachmentSetIds: readonly string[];
        }) => db.prepare(`SELECT id,purpose_key,component_kind,component_ref FROM attachment_sets WHERE company_id=? AND id IN (${facts.attachmentSetIds.map(() => "?").join(",")})`).bind(facts.companyId, ...facts.attachmentSetIds).all<{
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
