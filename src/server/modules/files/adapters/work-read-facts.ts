import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import { attachmentsV2ReadOnly } from "./attachments/feature";
export function workReadFilesFacts(db: SqlDatabase) {
    return { evidenceSets: (facts: {
            recordId: string;
            companyId: string;
        }) => db.prepare(`SELECT link.id,link.set_id,set_row.purpose_key,
             COALESCE(source.original_name,intent.original_name,'Pending file') original_name,
             COALESCE(source.bytes,intent.actual_bytes,intent.declared_bytes) bytes,
             COALESCE(source.detected_media_type,source.media_type,intent.detected_media_type,intent.media_type) media_type,
             source.source_state,link.state link_state,
             (source.preview_object_key IS NOT NULL) preview_available,
             link.created_at
           FROM attachment_sets set_row
           JOIN attachment_links link ON link.set_id=set_row.id
           LEFT JOIN attachments source
             ON source.id=link.attachment_id AND source.company_id=set_row.company_id
           LEFT JOIN upload_intents intent
             ON intent.id=link.upload_intent_id AND intent.company_id=set_row.company_id
           WHERE set_row.target_type='leave_request' AND set_row.target_id=?
             AND set_row.company_id=?
             AND (link.attachment_id IS NULL OR source.id IS NOT NULL)
             AND (link.upload_intent_id IS NULL OR intent.id IS NOT NULL)
             AND set_row.state${attachmentsV2ReadOnly() ? "='bound'" : " IN ('draft','bound')"}
             AND link.state${attachmentsV2ReadOnly() ? "='bound'" : " IN ('draft','bound')"}
             ${attachmentsV2ReadOnly() ? "AND source.source_state='available'" : ""}
           ORDER BY link.created_at DESC,link.id DESC`).bind(facts.recordId, facts.companyId).all<{
            id: string;
            set_id: string;
            purpose_key: string;
            original_name: string;
            bytes: number | null;
            media_type: string | null;
            source_state: string | null;
            link_state: string;
            preview_available: boolean;
            created_at: number;
        }>(),
        legacyEvidence: (facts: {
            recordId: string;
            companyId: string;
        }) => db.prepare("SELECT id,NULL::text set_id,purpose purpose_key,original_name,bytes,COALESCE(detected_media_type,media_type) media_type,source_state,'bound' link_state,(preview_object_key IS NOT NULL) preview_available,created_at FROM attachments WHERE owner_type='leave_request' AND owner_id=? AND company_id=? AND revoked_at IS NULL ORDER BY created_at DESC").bind(facts.recordId, facts.companyId).all<{
            id: string;
            original_name: string;
            bytes: number;
            media_type: string;
            set_id: string | null;
            purpose_key: string;
            source_state: string;
            link_state: string;
            preview_available: boolean;
            created_at: number;
        }>() };
}
