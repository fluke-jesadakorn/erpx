import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import { nativeFilesRead } from "./native-files-command";

/**
 * Legal clean-source attachment fact. The read is owned by the native Files
 * read kind `RD_LEGAL_DOCUMENT_CLEAN_SOURCE`; this module emits no SQL and
 * fails closed without the native transport. The caller's `.first()` row
 * shape is preserved.
 */
export function legalDocumentSourceFacts(db:SqlDatabase){return {
cleanSource:(facts:{sourceId:string;companyId:string;})=>nativeFilesRead<{
        object_key: string;
        checksum_sha256: string;
        media_type: string;
        bytes: number;
      }>('RD_LEGAL_DOCUMENT_CLEAN_SOURCE',[facts.sourceId,facts.companyId],db,facts.companyId)
};}
