import 'server-only';
import type { SqlDatabase } from '@/server/modules/runtime/public/runtime/types';
import type { WorkContext } from '@/server/modules/identity_access/public/authorization';
import { assertion } from '@/server/modules/workflow/public/commands';
export async function taxSignatureMetadata(db: SqlDatabase, actor: WorkContext, id: string, linkId: string) {
    return db.prepare(`SELECT a.checksum_sha256,a.media_type,a.bytes FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN attachments a ON a.id=l.attachment_id
      WHERE l.id=? AND s.company_id=? AND s.target_type='finance_tax_identity' AND s.target_id=? AND s.purpose_key='signer_signature' AND s.component_kind='record' AND s.state='bound' AND l.state='bound' AND a.company_id=s.company_id AND a.source_state='available'`).bind(linkId, actor.companyId, id).first<{
        checksum_sha256: string;
        media_type: string;
        bytes: number;
    }>();
}
export function taxSignatureGuard(db: SqlDatabase, actor: WorkContext, linkId: string, id: string, sha256: string, commandId: string, tag: string) { return [assertion(db, commandId, tag, "EXISTS(SELECT 1 FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN attachments a ON a.id=l.attachment_id WHERE l.id=? AND s.company_id=? AND s.target_id=? AND s.target_type='finance_tax_identity' AND l.state='bound' AND s.state='bound' AND a.source_state='available' AND a.checksum_sha256=?)", [linkId, actor.companyId, id, sha256])]; }
export async function taxSignatureWorkspace(db: SqlDatabase, actor: WorkContext) { return (await db.prepare("SELECT l.id,s.target_id,a.original_name,a.checksum_sha256 FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN attachments a ON a.id=l.attachment_id WHERE s.company_id=? AND s.target_type='finance_tax_identity' AND s.purpose_key='signer_signature' AND s.component_kind='record' AND s.state='bound' AND l.state='bound' AND a.source_state='available'").bind(actor.companyId).all<Record<string, string | number | boolean | null>>()).results; }
