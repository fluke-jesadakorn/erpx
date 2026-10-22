import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function legalPreservationSource(db:SqlDatabase,companyId:string,attachmentId:string){
    return db
      .prepare(
        `SELECT a.*,(SELECT l.id FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id
          WHERE l.attachment_id=a.id AND s.company_id=a.company_id AND s.target_type=a.owner_type AND s.target_id=a.owner_id
            AND l.state IN ('draft','bound') ORDER BY CASE l.state WHEN 'bound' THEN 0 ELSE 1 END,l.created_at LIMIT 1) attachment_link_id
         FROM attachments a WHERE a.id=? AND a.company_id=? AND a.revoked_at IS NULL`,
      )
      .bind(attachmentId, companyId)
      .first<{
        object_key: string;
        media_type: string;
        original_name: string;
        owner_type: string;
        owner_id: string;
        checksum_sha256: string;
        source_state: string;
        attachment_link_id: string | null;
      }>();

}
