import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function pmsFrontAttachmentFacts(db: SqlDatabase) {
    return {
        setTarget: (id: string, companyId: string) => db.prepare('SELECT target_type FROM attachment_sets WHERE id=? AND company_id=?').bind(id, companyId).first<{
            target_type: string;
        }>(),
        linkSet: (id: string, companyId: string) => db.prepare('SELECT l.set_id FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id WHERE l.id=? AND s.company_id=?').bind(id, companyId).first<{
            set_id: string;
        }>(),
        upload: (id: string, companyId: string, userId: string) => db.prepare('SELECT attachment_set_id,declared_bytes,media_type FROM upload_intents WHERE id=? AND company_id=? AND created_by=?').bind(id, companyId, userId).first<{
            attachment_set_id: string;
            declared_bytes: number;
            media_type: string;
        }>()
    };
}
