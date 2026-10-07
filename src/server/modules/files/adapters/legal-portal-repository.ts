import "server-only";
import type {SqlDatabase,SqlStatement} from "../../runtime/public/runtime/types";
import {PostgresDatabase} from "../../runtime/public/runtime/postgres";
import {legalPortalStorageAllowance} from "../../manage_erpx/public/legal-portal-storage-allowance";
import type {PortalAttachmentWrite,PortalAttachmentUnit,PortalAttachmentRepository} from "../../legal/public/portal-attachment-contracts";
function unit(db:SqlDatabase):PortalAttachmentUnit{const writes=new WeakMap<PortalAttachmentWrite,SqlStatement>();function stage(statement:SqlStatement):PortalAttachmentWrite{const write:PortalAttachmentWrite={owner:"files"};writes.set(write,statement);return write;}
return {
existing:(facts:{sourceId:string;companyId:string;})=>db.prepare("SELECT object_key,checksum_sha256,media_type,bytes FROM attachments WHERE id=? AND company_id=?").bind(facts.sourceId,facts.companyId).first<{
      object_key: string;
      checksum_sha256: string;
      media_type: string;
      bytes: number;
    }>(),
usedStorage:(facts:{companyId:string;})=>db.prepare("SELECT COALESCE(sum(bytes),0) FROM attachment_storage_ledger WHERE company_id=? AND state IN ('reserved','stored')").bind(facts.companyId).first<number>("coalesce"),
pendingStorage:(facts:{companyId:string;now:number;})=>db.prepare("SELECT COALESCE(sum(declared_bytes),0) FROM upload_intents WHERE company_id=? AND status='pending' AND expires_at>?").bind(facts.companyId,facts.now).first<number>("coalesce"),
storageAllowance:facts=>legalPortalStorageAllowance(db,facts.companyId),
raced:(facts:{sourceId:string;})=>db.prepare("SELECT checksum_sha256 FROM attachments WHERE id=?").bind(facts.sourceId).first<{ checksum_sha256: string }>(),
draftSet:(facts:{setId:string;companyId:string;branchId:string | null;departmentId:string | null;fact4:"legal_item";itemId:string;fact6:"action";commandId:string;PORTAL_PURPOSE:"legal_portal_submission";itemVersion:number;version:number;classification:"public"|"internal"|"confidential"|"restricted";fact12:string;retentionPolicyKey:string | null;requiredCount:number;maxFiles:number;actorId:string;now:number;now2:number;})=>stage(db.prepare(`INSERT INTO attachment_sets(
              id,company_id,branch_id,department_id,target_type,target_id,component_kind,component_ref,purpose_key,
              expected_target_version,policy_version,classification,audience_json,retention_policy_key,
              required_count,max_files,state,created_by,created_at,updated_at
            ) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'draft',?,?,?)`).bind(facts.setId,facts.companyId,facts.branchId,facts.departmentId,facts.fact4,facts.itemId,facts.fact6,facts.commandId,facts.PORTAL_PURPOSE,facts.itemVersion,facts.version,facts.classification,facts.fact12,facts.retentionPolicyKey,facts.requiredCount,facts.maxFiles,facts.actorId,facts.now,facts.now2)),
draftLink:(facts:{linkId:string;setId:string;actorId:string;now:number;fact4:number;})=>stage(db.prepare("INSERT INTO attachment_links(id,set_id,state,created_by,created_at,expires_at) VALUES(?,?,'draft',?,?,?)").bind(facts.linkId,facts.setId,facts.actorId,facts.now,facts.fact4)),
uploadedIntent:(facts:{uploadId:string;fact1:"legal_item";itemId:string;PORTAL_PURPOSE:"legal_portal_submission";name:string;mediaType:"application/pdf" | "image/png" | "image/jpeg" | "image/webp" | "image/heic" | "image/heif" | "application/vnd.openxmlformats-officedocument.wordprocessingml.document" | "text/csv";byteLength:number;byteLength2:number;checksum:string;objectKey:string;objectKey2:string;actorId:string;fact12:number;now:number;now2:number;itemVersion:number;setId:string;linkId:string;companyId:string;mediaType2:"application/pdf" | "image/png" | "image/jpeg" | "image/webp" | "image/heic" | "image/heif" | "application/vnd.openxmlformats-officedocument.wordprocessingml.document" | "text/csv";objectKey3:string;})=>stage(db.prepare(`INSERT INTO upload_intents(
              id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,
              staging_key,final_key,status,created_by,expires_at,created_at,updated_at,owner_version,
              attachment_set_id,attachment_link_id,company_id,detected_media_type,quarantine_key
            ) VALUES(?,?,?,?,?,?,?,?,?,?,?,'uploaded',?,?,?,?,?,?,?,?,?,?)`).bind(facts.uploadId,facts.fact1,facts.itemId,facts.PORTAL_PURPOSE,facts.name,facts.mediaType,facts.byteLength,facts.byteLength2,facts.checksum,facts.objectKey,facts.objectKey2,facts.actorId,facts.fact12,facts.now,facts.now2,facts.itemVersion,facts.setId,facts.linkId,facts.companyId,facts.mediaType2,facts.objectKey3)),
quarantinedSource:(facts:{sourceId:string;uploadId:string;fact2:"legal_item";itemId:string;PORTAL_PURPOSE:"legal_portal_submission";objectKey:string;name:string;mediaType:"application/pdf" | "image/png" | "image/jpeg" | "image/webp" | "image/heic" | "image/heif" | "application/vnd.openxmlformats-officedocument.wordprocessingml.document" | "text/csv";byteLength:number;checksum:string;actorId:string;now:number;companyId:string;mediaType2:"application/pdf" | "image/png" | "image/jpeg" | "image/webp" | "image/heic" | "image/heif" | "application/vnd.openxmlformats-officedocument.wordprocessingml.document" | "text/csv";})=>stage(db.prepare(`INSERT INTO attachments(
              id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,
              checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state
            ) VALUES(?,?,?,?,?,?,1,?,?,?,?,?,?,?,?,'quarantined')`).bind(facts.sourceId,facts.uploadId,facts.fact2,facts.itemId,facts.PORTAL_PURPOSE,facts.objectKey,facts.name,facts.mediaType,facts.byteLength,facts.checksum,facts.actorId,facts.now,facts.companyId,facts.mediaType2)),
linkUpload:(facts:{uploadId:string;sourceId:string;linkId:string;})=>stage(db.prepare("UPDATE attachment_links SET upload_intent_id=?,attachment_id=? WHERE id=?").bind(facts.uploadId,facts.sourceId,facts.linkId)),
pendingScan:(facts:{fact0:string;sourceId:string;now:number;})=>stage(db.prepare("INSERT INTO attachment_scan_attempts(id,attachment_id,attempt,state,created_at) VALUES(?,?,1,'pending',?)").bind(facts.fact0,facts.sourceId,facts.now)),
storedOriginal:(facts:{fact0:string;companyId:string;sourceId:string;objectKey:string;byteLength:number;now:number;})=>stage(db.prepare("INSERT INTO attachment_storage_ledger(id,company_id,attachment_id,variant,object_key,bytes,state,created_at) VALUES(?,?,?,'original',?,?,'stored',?)").bind(facts.fact0,facts.companyId,facts.sourceId,facts.objectKey,facts.byteLength,facts.now)),
quarantineEvent:(facts:{fact0:string;companyId:string;sourceId:string;linkId:string;setId:string;actorId:string;fact6:"file.quarantined";fact7:string;now:number;})=>stage(db.prepare("INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,action,details_json,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(facts.fact0,facts.companyId,facts.sourceId,facts.linkId,facts.setId,facts.actorId,facts.fact6,facts.fact7,facts.now)),
readableSource:(facts:{linkId:string;companyId:string;itemId:string;purposeKey:"legal_document";})=>db.prepare(`SELECT l.id AS link_id,l.attachment_id,a.object_key,a.media_type,a.original_name,a.checksum_sha256,a.source_state
       FROM attachment_links l
       JOIN attachment_sets s ON s.id=l.set_id
       JOIN attachments a ON a.id=l.attachment_id
       WHERE l.id=? AND l.state='bound' AND s.company_id=? AND s.target_type='legal_item'
         AND s.target_id=? AND s.purpose_key=? AND a.source_state='available'`).bind(facts.linkId,facts.companyId,facts.itemId,facts.purposeKey).first<{
      link_id: string;
      attachment_id: string;
      object_key: string;
      media_type: string;
      original_name: string;
      checksum_sha256: string;
      source_state: string;
    }>(),
allowedRead:(facts:{fact0:string;companyId:string;attachment_id:string;link_id:string;actorId:string;now:number;})=>db.prepare("INSERT INTO attachment_access_events(id,company_id,attachment_id,link_id,actor_id,variant,outcome,created_at) VALUES(?,?,?,?,?,'original','allowed',?)").bind(facts.fact0,facts.companyId,facts.attachment_id,facts.link_id,facts.actorId,facts.now).run(),
deniedRead:(facts:{fact0:string;companyId:string;attachment_id:string;link_id:string;actorId:string;fact5:string;now:number;})=>db.prepare("INSERT INTO attachment_access_events(id,company_id,attachment_id,link_id,actor_id,variant,outcome,reason,created_at) VALUES(?,?,?,?,?,'original','denied',?,?)").bind(facts.fact0,facts.companyId,facts.attachment_id,facts.link_id,facts.actorId,facts.fact5,facts.now).run(),batch:steps=>db.batch(steps.map(write=>{const statement=writes.get(write);if(!statement)throw new Error("LEGAL_PORTAL_FILES_REPOSITORY_MISMATCH");return statement;})),};}
export function legalPortalRepository(db:SqlDatabase):PortalAttachmentRepository{return {...unit(db),transaction:work=>{if(!(db instanceof PostgresDatabase))throw new Error("LEGAL_PORTAL_FILES_TRANSACTION_REQUIRED");return db.transaction(tx=>work(unit(tx)));}};}
