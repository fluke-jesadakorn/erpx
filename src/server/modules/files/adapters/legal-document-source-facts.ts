import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
export function legalDocumentSourceFacts(db:SqlDatabase){return {
cleanSource:(facts:{sourceId:string;companyId:string;})=>db.prepare("SELECT object_key,checksum_sha256,media_type,bytes FROM attachments WHERE id=? AND company_id=? AND source_state='available'").bind(facts.sourceId,facts.companyId).first<{
        object_key: string;
        checksum_sha256: string;
        media_type: string;
        bytes: number;
      }>()
};}
