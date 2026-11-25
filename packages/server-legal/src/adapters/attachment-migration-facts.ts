import { nativeBackfillAttachment } from "@erpx/server-collaboration/adapters/native-backfill-attachment";
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {legalExternalUploadShareJoinHead} from "./legal-read-tail-r44-sources";
import contract from "./attachment-migration-facts-sources.json" with { type: "json" };
// One shared fixed-SQL hold witness for both specialized-attachment backfill reads.
const LEGAL_HOLD_EXISTS = contract.holdExists;
const LEGAL_EXTERNAL_PROJECTION = `u.id,i.company_id,i.branch_id,i.department_id,i.id target_id,i.version target_version,u.id component_ref,
    u.name original_name,u.media_type,u.bytes,u.checksum,u.object_key,u.created_by,u.created_at,
    ${LEGAL_HOLD_EXISTS}
   `;
export function attachmentMigrationlegal(database:SqlDatabase){return {readBackfillSpecializedAttachmentsDocuments(){return database.prepare(contract.documents.replace("{holdExists}", () => LEGAL_HOLD_EXISTS)).all<Record<string,unknown>>();},
readBackfillSpecializedAttachmentsExternal(){return database.prepare(`${legalExternalUploadShareJoinHead(LEGAL_EXTERNAL_PROJECTION,'u','s')}JOIN legal_items i ON i.id=s.item_id
    WHERE u.attachment_source_id IS NULL ORDER BY u.created_at,u.id`).all<Record<string,unknown>>();},
stageBackfillSpecializedAttachmentsEffect11(linkId:string,attachmentId:string,id:string,companyId:string){return nativeBackfillAttachment(database,companyId).legalDocument(linkId,attachmentId,id);},
stageBackfillSpecializedAttachmentsEffect12(linkId:string,attachmentId:string,id:string,companyId:string){return nativeBackfillAttachment(database,companyId).legalExternal(linkId,attachmentId,id);}};}
