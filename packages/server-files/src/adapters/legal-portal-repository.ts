import "server-only";
import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import {PostgresDatabase} from "@erpx/server-runtime/public/runtime/postgres";
import {legalPortalStorageAllowance} from "@erpx/server-manage_erpx/public/legal-portal-storage-allowance";
import type {PortalAttachmentWrite,PortalAttachmentUnit,PortalAttachmentRepository,PortalAttachmentStageScope,PortalAttachmentRecoverySource} from "@erpx/server-legal/public/portal-attachment-contracts";
import { legalAccessEvents } from '@erpx/server-collaboration/adapters/native-legal-access-events';
import { legalPortalStagedStatements } from '@erpx/server-collaboration/adapters/native-legal-portal-staged';
function unit(db:SqlDatabase):PortalAttachmentUnit{const nativeAccess=legalAccessEvents(db),nativeStaged=legalPortalStagedStatements(db);const writes=new WeakMap<PortalAttachmentWrite,SqlStatement>();function stage(statement:SqlStatement):PortalAttachmentWrite{const write:PortalAttachmentWrite={owner:"files"};writes.set(write,statement);return write;}
function stagedSource(facts:PortalAttachmentStageScope){return db.prepare(`SELECT a.object_key,a.checksum_sha256,a.media_type,a.bytes,s.expected_target_version,
 CASE WHEN s.state='draft' AND l.state='draft' AND i.status='ready' AND a.source_state='available' AND a.revoked_at IS NULL THEN 1 ELSE 0 END recovery_draft,
 jsonb_build_object('source',to_jsonb(a),'upload',to_jsonb(i),'link',to_jsonb(l),'set',to_jsonb(s))::text image,
 CASE WHEN a.owner_type='legal_item' AND a.owner_id=? AND a.purpose='legal_portal_submission' AND a.created_by=? AND a.object_key=? AND a.original_name=?
  AND i.id=? AND i.company_id=a.company_id AND i.owner_type=a.owner_type AND i.owner_id=a.owner_id AND i.purpose=a.purpose
  AND i.created_by=a.created_by AND i.owner_version=s.expected_target_version AND i.final_key=a.object_key AND i.original_name=a.original_name
  AND i.attachment_set_id=s.id AND i.attachment_link_id=l.id
  AND l.id=? AND l.created_by=a.created_by AND l.attachment_id=a.id
  AND s.id=? AND s.company_id=a.company_id AND s.target_type=a.owner_type AND s.target_id=a.owner_id
  AND s.component_kind='action' AND s.component_ref=? AND s.purpose_key=a.purpose AND s.created_by=a.created_by
  AND s.audience_json::jsonb->>'portalShareId'=? THEN 1 ELSE 0 END scope_matches
 FROM attachments a LEFT JOIN upload_intents i ON i.id=a.upload_id
 LEFT JOIN attachment_links l ON l.id=i.attachment_link_id LEFT JOIN attachment_sets s ON s.id=l.set_id
 WHERE a.id=? AND a.company_id=?`).bind(facts.itemId,facts.actorId,facts.objectKey,facts.name,facts.uploadId,facts.linkId,facts.setId,facts.commandId,facts.shareId,facts.sourceId,facts.companyId).first<PortalAttachmentRecoverySource>();}
return {
existing:stagedSource,
recoverySource:stagedSource,
usedStorage:(facts:{companyId:string;})=>db.prepare("SELECT COALESCE(sum(bytes),0) FROM attachment_storage_ledger WHERE company_id=? AND state IN ('reserved','stored')").bind(facts.companyId).first<number>("coalesce"),
pendingStorage:(facts:{companyId:string;now:number;})=>db.prepare("SELECT COALESCE(sum(declared_bytes),0) FROM upload_intents WHERE company_id=? AND status='pending' AND expires_at>?").bind(facts.companyId,facts.now).first<number>("coalesce"),
storageAllowance:facts=>legalPortalStorageAllowance(db,facts.companyId),
raced:stagedSource,
async draftSet(facts){return stage(await nativeStaged.draftSet(facts));},
async draftLink(facts){return stage(await nativeStaged.draftLink(facts));},
async uploadedIntent(facts){return stage(await nativeStaged.uploadedIntent(facts));},
async quarantinedSource(facts){return stage(await nativeStaged.quarantinedSource(facts));},
async linkUpload(facts){return stage(await nativeStaged.linkUpload(facts));},
async pendingScan(facts){return stage(await nativeStaged.pendingScan(facts));},
async storedOriginal(facts){return stage(await nativeStaged.storedOriginal(facts));},
async quarantineEvent(facts){return stage(await nativeStaged.quarantineEvent(facts));},
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
allowedRead:facts=>nativeAccess.allowedRead(facts),
deniedRead:facts=>nativeAccess.deniedRead(facts),batch:steps=>db.batch(steps.map(write=>{const statement=writes.get(write);if(!statement)throw new Error("LEGAL_PORTAL_FILES_REPOSITORY_MISMATCH");return statement;})),};}
export function legalPortalRepository(db:SqlDatabase):PortalAttachmentRepository{return {...unit(db),transaction:work=>{if(!(db instanceof PostgresDatabase))throw new Error("LEGAL_PORTAL_FILES_TRANSACTION_REQUIRED");return db.transaction(tx=>work(unit(tx)));}};}
