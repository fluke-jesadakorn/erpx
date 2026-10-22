import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function legalCorpusSourceFacts(db:SqlDatabase){return {
cleanSourceLink:(facts:{attachment_link_id:string | null | undefined;companyId:string;checksum:string;})=>db.prepare("SELECT 1 FROM attachment_links l JOIN attachments f ON f.id=l.attachment_id WHERE l.id=? AND l.state='bound' AND f.company_id=? AND f.source_state='available' AND f.revoked_at IS NULL AND f.checksum_sha256=?").bind(facts.attachment_link_id,facts.companyId,facts.checksum).first()
};}
