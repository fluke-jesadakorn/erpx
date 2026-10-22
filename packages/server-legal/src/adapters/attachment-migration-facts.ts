import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';export function attachmentMigrationlegal(database:SqlDatabase){return {readBackfillSpecializedAttachmentsDocuments(){return database.prepare(`SELECT d.id,i.company_id,i.branch_id,i.department_id,i.id target_id,i.version target_version,d.id component_ref,
    d.name original_name,d.media_type,d.bytes,d.checksum,d.object_key,d.created_by,d.created_at,d.version revision,
    EXISTS(SELECT 1 FROM legal_holds h WHERE h.item_id=i.id AND h.state<>'released') legal_hold
    FROM legal_documents d JOIN legal_items i ON i.id=d.item_id WHERE d.object_key IS NOT NULL AND d.attachment_source_id IS NULL ORDER BY d.created_at,d.id`).all<Record<string,unknown>>();},
readBackfillSpecializedAttachmentsExternal(){return database.prepare(`SELECT u.id,i.company_id,i.branch_id,i.department_id,i.id target_id,i.version target_version,u.id component_ref,
    u.name original_name,u.media_type,u.bytes,u.checksum,u.object_key,u.created_by,u.created_at,
    EXISTS(SELECT 1 FROM legal_holds h WHERE h.item_id=i.id AND h.state<>'released') legal_hold
    FROM legal_external_uploads u JOIN legal_shares s ON s.id=u.share_id JOIN legal_items i ON i.id=s.item_id
    WHERE u.attachment_source_id IS NULL ORDER BY u.created_at,u.id`).all<Record<string,unknown>>();},
stageBackfillSpecializedAttachmentsEffect11(linkId:string,attachmentId:string,id:string){return database.prepare("UPDATE legal_documents SET attachment_link_id=?,attachment_source_id=? WHERE id=? AND attachment_source_id IS NULL").bind(linkId,attachmentId,id);},
stageBackfillSpecializedAttachmentsEffect12(linkId:string,attachmentId:string,id:string){return database.prepare("UPDATE legal_external_uploads SET attachment_link_id=?,attachment_source_id=? WHERE id=? AND attachment_source_id IS NULL").bind(linkId,attachmentId,id);}};}
