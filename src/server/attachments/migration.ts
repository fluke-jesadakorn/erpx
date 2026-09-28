import "server-only";

import { createHash } from "node:crypto";
import type { FileStore, SqlDatabase } from "../runtime/types";
import { sha256 } from "../crypto";
import { attachmentPolicy,attachmentRetentionSnapshot } from "./policy";
import { legacyAttachmentPolicy } from "./legacy-policy";
import type { AttachmentComponentKind } from "./types";

type LegacySource={id:string;upload_id:string;owner_type:string;owner_id:string;purpose:string;object_key:string;original_name:string;media_type:string;bytes:number;checksum_sha256:string;created_by:string;created_at:number};
type Scope={companyId:string;branchId:string|null;departmentId:string|null;version:number};
type SpecializedSource={kind:"messaging"|"legal_document"|"legal_external";id:string;companyId:string;branchId:string|null;departmentId:string|null;targetType:"messaging_room"|"legal_item";targetId:string;targetVersion:number;componentKind:AttachmentComponentKind;componentRef:string;purposeKey:string;originalName:string;mediaType:string;bytes:number;checksum:string;objectKey:string;createdBy:string;createdAt:number;revision:number;bound:boolean;legalHold:boolean};
export type AttachmentUnavailableSource={id:string;companyId:string|null;ownerType:string;ownerId:string;purpose:string;mediaType:string;bytes:number;sourceState:string;scanAttempt:number|null;scanState:string|null;scanResult:string|null;scanError:string|null};

async function legacyScope(db:SqlDatabase,source:LegacySource):Promise<Scope|null>{
  const simple:Record<string,{table:string;branch?:string;department?:string}>={
    support_ticket:{table:"support_tickets",branch:"branch_id",department:"department_id"},employee_request:{table:"employee_requests",branch:"branch_id",department:"department_id"},leave_request:{table:"leave_requests",branch:"branch_id",department:"department_id"},expense_claim:{table:"finance_expenses",branch:"branch_id",department:"department_id"},invoice_payment:{table:"finance_invoices"},staff_task:{table:"staff_tasks",branch:"branch_id",department:"department_id"},social_draft:{table:"social_post_drafts",branch:"branch_id",department:"department_id"},quality_hold:{table:"inventory_quality_holds",branch:"branch_id"},commerce_order_delivery:{table:"commerce_orders",branch:"branch_id",department:"department_id"},
  };
  if(source.owner_type==="expense_payment"){
    const row=await db.prepare("SELECT e.company_id,e.branch_id,e.department_id,d.version FROM expense_payment_drafts d JOIN finance_expenses e ON e.id=d.expense_id WHERE d.id=?").bind(source.owner_id).first<{company_id:string;branch_id:string|null;department_id:string|null;version:number}>();
    return row?{companyId:row.company_id,branchId:row.branch_id,departmentId:row.department_id,version:Number(row.version)}:null;
  }
  if(source.owner_type==="branch_brand"){
    const row=await db.prepare("SELECT company_id,id branch_id FROM organization_nodes WHERE id=? AND kind='branch' AND status='active'").bind(source.owner_id).first<{company_id:string;branch_id:string}>();
    return row?{companyId:row.company_id,branchId:row.branch_id,departmentId:null,version:1}:null;
  }
  if(source.owner_type==="company_storefront_brand"){
    const row=await db.prepare("SELECT company_id FROM organization_nodes WHERE id=? AND kind='company' AND status='active'").bind(source.owner_id).first<{company_id:string}>();
    return row?{companyId:row.company_id,branchId:null,departmentId:null,version:1}:null;
  }
  if(source.owner_type==="holding_brand"){
    const companies=(await db.prepare("SELECT DISTINCT company_id FROM organization_nodes WHERE kind='company' AND status='active' ORDER BY company_id LIMIT 2").all<{company_id:string}>()).results;
    return companies.length===1?{companyId:companies[0]!.company_id,branchId:null,departmentId:null,version:1}:null;
  }
  const descriptor=simple[source.owner_type];if(!descriptor)return null;
  const row=await db.prepare(`SELECT company_id,${descriptor.branch??"NULL"} branch_id,${descriptor.department??"NULL"} department_id,version FROM ${descriptor.table} WHERE id=?`).bind(source.owner_id).first<{company_id:string;branch_id:string|null;department_id:string|null;version:number}>();
  return row?{companyId:row.company_id,branchId:row.branch_id,departmentId:row.department_id,version:Number(row.version)}:null;
}

export async function backfillSpecializedAttachments(db:SqlDatabase,{apply=false}:{apply?:boolean}={}){
  const messaging=(await db.prepare(`SELECT f.id,r.company_id,NULL::text branch_id,NULL::text department_id,r.id target_id,r.version target_version,
    COALESCE(f.message_id,f.id) component_ref,f.original_name,f.media_type,f.bytes,f.checksum,f.object_key,f.created_by,f.created_at,
    CASE WHEN f.message_id IS NULL THEN false ELSE true END bound
    FROM messaging_files f JOIN messaging_rooms r ON r.id=f.room_id WHERE f.attachment_source_id IS NULL ORDER BY f.created_at,f.id`).all<Record<string,unknown>>()).results.map(row=>({kind:"messaging" as const,id:String(row.id),companyId:String(row.company_id),branchId:null,departmentId:null,targetType:"messaging_room" as const,targetId:String(row.target_id),targetVersion:Number(row.target_version),componentKind:"message" as const,componentRef:String(row.component_ref),purposeKey:"message_attachment",originalName:String(row.original_name),mediaType:String(row.media_type),bytes:Number(row.bytes),checksum:String(row.checksum),objectKey:String(row.object_key),createdBy:String(row.created_by),createdAt:Number(row.created_at),revision:1,bound:Boolean(row.bound),legalHold:false}));
  const documents=(await db.prepare(`SELECT d.id,i.company_id,i.branch_id,i.department_id,i.id target_id,i.version target_version,d.id component_ref,
    d.name original_name,d.media_type,d.bytes,d.checksum,d.object_key,d.created_by,d.created_at,d.version revision,
    EXISTS(SELECT 1 FROM legal_holds h WHERE h.item_id=i.id AND h.state<>'released') legal_hold
    FROM legal_documents d JOIN legal_items i ON i.id=d.item_id WHERE d.object_key IS NOT NULL AND d.attachment_source_id IS NULL ORDER BY d.created_at,d.id`).all<Record<string,unknown>>()).results.map(row=>({kind:"legal_document" as const,id:String(row.id),companyId:String(row.company_id),branchId:row.branch_id?String(row.branch_id):null,departmentId:row.department_id?String(row.department_id):null,targetType:"legal_item" as const,targetId:String(row.target_id),targetVersion:Number(row.target_version),componentKind:"record" as const,componentRef:String(row.component_ref),purposeKey:"legal_document",originalName:String(row.original_name),mediaType:String(row.media_type),bytes:Number(row.bytes),checksum:String(row.checksum),objectKey:String(row.object_key),createdBy:String(row.created_by),createdAt:Number(row.created_at),revision:Number(row.revision),bound:true,legalHold:Boolean(row.legal_hold)}));
  const external=(await db.prepare(`SELECT u.id,i.company_id,i.branch_id,i.department_id,i.id target_id,i.version target_version,u.id component_ref,
    u.name original_name,u.media_type,u.bytes,u.checksum,u.object_key,u.created_by,u.created_at,
    EXISTS(SELECT 1 FROM legal_holds h WHERE h.item_id=i.id AND h.state<>'released') legal_hold
    FROM legal_external_uploads u JOIN legal_shares s ON s.id=u.share_id JOIN legal_items i ON i.id=s.item_id
    WHERE u.attachment_source_id IS NULL ORDER BY u.created_at,u.id`).all<Record<string,unknown>>()).results.map(row=>({kind:"legal_external" as const,id:String(row.id),companyId:String(row.company_id),branchId:row.branch_id?String(row.branch_id):null,departmentId:row.department_id?String(row.department_id):null,targetType:"legal_item" as const,targetId:String(row.target_id),targetVersion:Number(row.target_version),componentKind:"action" as const,componentRef:String(row.component_ref),purposeKey:"legal_portal_submission",originalName:String(row.original_name),mediaType:String(row.media_type),bytes:Number(row.bytes),checksum:String(row.checksum),objectKey:String(row.object_key),createdBy:String(row.created_by),createdAt:Number(row.created_at),revision:1,bound:true,legalHold:Boolean(row.legal_hold)}));
  const sources:SpecializedSource[]=[...messaging,...documents,...external],report={examined:sources.length,ready:0,backfilled:0,unknown:[] as Array<{kind:string;id:string;reason:string}>};
  for(const source of sources){
    let policy;try{policy=attachmentPolicy(source.targetType,source.purposeKey,source.componentKind);}catch{report.unknown.push({kind:source.kind,id:source.id,reason:"POLICY_UNKNOWN"});continue;}
    if(!policy.mediaTypes.includes(source.mediaType)||source.bytes<1||source.bytes>policy.maxBytes){report.unknown.push({kind:source.kind,id:source.id,reason:"FORMAT_OUTSIDE_POLICY"});continue;}
    report.ready++;if(!apply)continue;
    const digest=createHash("sha256").update(`${source.kind}\0${source.id}`).digest("hex").slice(0,32),setId=`attachment-set-specialized-${digest}`,linkId=`attachment-link-specialized-${digest}`,uploadId=`attachment-upload-specialized-${digest}`,attachmentId=`attachment-specialized-${digest}`,scanId=`attachment-scan-specialized-${digest}`,now=Date.now(),retention=attachmentRetentionSnapshot(policy),expiresAt=source.createdAt+7*24*60*60_000,boundAt=source.bound?source.createdAt:null;
    await db.batch([
      db.prepare(`INSERT INTO attachment_sets(id,company_id,branch_id,department_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,retention_policy_key,retention_policy_version,retain_until,destruction_mode,required_count,max_files,state,created_by,created_at,updated_at,bound_at,bound_command_id)
        VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'draft',?,?,?,NULL,NULL) ON CONFLICT(id) DO NOTHING`).bind(setId,source.companyId,source.branchId,source.departmentId,source.targetType,source.targetId,source.componentKind,source.componentRef,source.purposeKey,source.targetVersion,policy.version,policy.classification,JSON.stringify({classification:policy.classification,policyVersion:policy.version,legacySpecialized:true}),retention.key,retention.version,retention.retainUntil,retention.destructionMode,policy.requiredCount,policy.maxFiles,source.createdBy,source.createdAt,now),
      db.prepare(`INSERT INTO attachment_links(id,set_id,state,created_by,created_at,expires_at) VALUES(?,?,'draft',?,?,?) ON CONFLICT(id) DO NOTHING`).bind(linkId,setId,source.createdBy,source.createdAt,expiresAt),
      db.prepare(`INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,revision,status,created_by,expires_at,created_at,updated_at,owner_version,attachment_set_id,attachment_link_id,company_id,quarantine_key)
        VALUES(?,?,?,?,?,?,?,?,?,?,?,?, 'uploaded',?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO NOTHING`).bind(uploadId,source.targetType,source.targetId,source.purposeKey,source.originalName,source.mediaType,source.bytes,source.bytes,source.checksum,source.objectKey,source.objectKey,source.revision,source.createdBy,expiresAt,source.createdAt,now,source.targetVersion,setId,linkId,source.companyId,source.objectKey),
      db.prepare("UPDATE attachment_links SET upload_intent_id=? WHERE id=? AND upload_intent_id IS NULL").bind(uploadId,linkId),
      db.prepare(`INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,legal_hold,created_at,company_id,source_state)
        VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?, ?,?,'quarantined') ON CONFLICT(id) DO NOTHING`).bind(attachmentId,uploadId,source.targetType,source.targetId,source.purposeKey,source.objectKey,source.revision,source.originalName,source.mediaType,source.bytes,source.checksum,source.createdBy,source.legalHold?1:0,source.createdAt,source.companyId),
      db.prepare("UPDATE attachment_links SET attachment_id=? WHERE id=? AND attachment_id IS NULL").bind(attachmentId,linkId),
      db.prepare("INSERT INTO attachment_scan_attempts(id,attachment_id,attempt,state,created_at) VALUES(?,?,1,'pending',?) ON CONFLICT(attachment_id,attempt) DO NOTHING").bind(scanId,attachmentId,now),
      db.prepare("INSERT INTO attachment_storage_ledger(id,company_id,attachment_id,variant,object_key,bytes,state,created_at) VALUES(?,?,?,'original',?,?,'stored',?) ON CONFLICT(object_key) DO NOTHING").bind(`attachment-storage-specialized-${digest}`,source.companyId,attachmentId,source.objectKey,source.bytes,source.createdAt),
      db.prepare("INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,action,details_json,created_at) VALUES(?,?,?,?,?,'system:migration','legacy.specialized_backfilled',?,?) ON CONFLICT(id) DO NOTHING").bind(`attachment-event-specialized-${digest}`,source.companyId,attachmentId,linkId,setId,JSON.stringify({kind:source.kind,legacyId:source.id}),now),
      source.kind==="messaging"
        ?db.prepare("UPDATE messaging_files SET attachment_set_id=?,attachment_link_id=?,attachment_source_id=? WHERE id=? AND attachment_source_id IS NULL").bind(setId,linkId,attachmentId,source.id)
        :source.kind==="legal_document"
          ?db.prepare("UPDATE legal_documents SET attachment_link_id=?,attachment_source_id=? WHERE id=? AND attachment_source_id IS NULL").bind(linkId,attachmentId,source.id)
          :db.prepare("UPDATE legal_external_uploads SET attachment_link_id=?,attachment_source_id=? WHERE id=? AND attachment_source_id IS NULL").bind(linkId,attachmentId,source.id),
      ...(source.bound?[
        db.prepare("UPDATE attachment_links SET state='bound',bound_at=?,binding_type=?,binding_id=? WHERE id=? AND state='draft'").bind(boundAt,`legacy_${source.kind}`,source.id,linkId),
        db.prepare("UPDATE attachment_sets SET state='bound',bound_at=?,bound_command_id=?,updated_at=? WHERE id=? AND state='draft'").bind(boundAt,`legacy-specialized-${source.id}`,now,setId),
      ]:[]),
    ]);report.backfilled++;
  }
  return report;
}

export async function backfillLegacyAttachments(db:SqlDatabase,{apply=false}:{apply?:boolean}={}){
  const sources=(await db.prepare("SELECT id,upload_id,owner_type,owner_id,purpose,object_key,original_name,media_type,bytes,checksum_sha256,created_by,created_at FROM attachments WHERE company_id IS NULL AND legacy_sanitized_only=false ORDER BY created_at,id").all<LegacySource>()).results;
  const report={examined:sources.length,ready:0,legacySanitizedOnly:[] as string[],unknown:[] as Array<{id:string;ownerType:string;ownerId:string;reason:string;targetCompanyId?:string}>,backfilled:0};
  for(const source of sources){
    const mapping=legacyAttachmentPolicy(source.owner_type,source.purpose),scope=await legacyScope(db,source);
    const unknown=(reason:string)=>({id:source.id,ownerType:source.owner_type,ownerId:source.owner_id,reason,...(scope?{targetCompanyId:scope.companyId}:{})});
    if(mapping&&source.owner_type==="holding_brand"&&!scope){report.legacySanitizedOnly.push(source.id);if(apply)await db.prepare("UPDATE attachments SET legacy_sanitized_only=true WHERE id=? AND company_id IS NULL AND source_state='legacy_unscanned'").bind(source.id).run();continue;}
    if(!mapping||!scope){report.unknown.push(unknown(!mapping?"POLICY_UNKNOWN":"OWNER_UNKNOWN"));if(apply)await db.prepare("UPDATE attachments SET source_state='quarantined' WHERE id=? AND company_id IS NULL AND source_state='legacy_unscanned'").bind(source.id).run();continue;}
    let policy;try{policy=attachmentPolicy(source.owner_type,mapping.purposeKey,mapping.componentKind);}catch{report.unknown.push(unknown("POLICY_UNKNOWN"));if(apply)await db.prepare("UPDATE attachments SET source_state='quarantined' WHERE id=? AND company_id IS NULL AND source_state='legacy_unscanned'").bind(source.id).run();continue;}
    const retention=attachmentRetentionSnapshot(policy);
    if(!policy.mediaTypes.includes(source.media_type)||source.bytes<1||source.bytes>policy.maxBytes){report.unknown.push(unknown("FORMAT_OUTSIDE_POLICY"));if(apply)await db.prepare("UPDATE attachments SET source_state='quarantined' WHERE id=? AND company_id IS NULL AND source_state='legacy_unscanned'").bind(source.id).run();continue;}
    report.ready++;if(!apply)continue;
    const digest=createHash("sha256").update(source.id).digest("hex").slice(0,32),setId=`attachment-set-legacy-${digest}`,linkId=`attachment-link-legacy-${digest}`,now=Date.now();
    await db.batch([
      db.prepare(`INSERT INTO attachment_sets(id,company_id,branch_id,department_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,retention_policy_key,retention_policy_version,retain_until,destruction_mode,required_count,max_files,state,created_by,created_at,updated_at,bound_at,bound_command_id)
        VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'bound',?,?,?,?,?) ON CONFLICT(id) DO NOTHING`).bind(setId,scope.companyId,scope.branchId,scope.departmentId,source.owner_type,source.owner_id,mapping.componentKind,mapping.componentRef,mapping.purposeKey,scope.version,policy.version,policy.classification,JSON.stringify({classification:policy.classification,policyVersion:policy.version,legacy:true}),retention.key,retention.version,retention.retainUntil,retention.destructionMode,policy.requiredCount,policy.maxFiles,source.created_by,source.created_at,now,now,`legacy-import-${source.id}`),
      db.prepare(`INSERT INTO attachment_links(id,set_id,attachment_id,upload_intent_id,state,created_by,created_at,expires_at,bound_at,binding_type,binding_id)
        VALUES(?,?,?,?,'bound',?,?,?,?, 'legacy_import',?) ON CONFLICT(id) DO NOTHING`).bind(linkId,setId,source.id,source.upload_id,source.created_by,source.created_at,source.created_at+7*24*60*60_000,now,source.owner_id),
      db.prepare("UPDATE attachments SET company_id=?,source_state='quarantined' WHERE id=? AND company_id IS NULL").bind(scope.companyId,source.id),
      db.prepare("UPDATE upload_intents SET attachment_set_id=?,attachment_link_id=?,company_id=?,quarantine_key=?,status='uploaded',updated_at=? WHERE id=?").bind(setId,linkId,scope.companyId,source.object_key,now,source.upload_id),
      db.prepare("INSERT INTO attachment_scan_attempts(id,attachment_id,attempt,state,created_at) VALUES(?,?,1,'pending',?) ON CONFLICT(attachment_id,attempt) DO NOTHING").bind(`attachment-scan-legacy-${digest}`,source.id,now),
      db.prepare("INSERT INTO attachment_storage_ledger(id,company_id,attachment_id,variant,object_key,bytes,state,created_at) VALUES(?,?,?,'original',?,?,'stored',?) ON CONFLICT(object_key) DO NOTHING").bind(`attachment-storage-legacy-${digest}`,scope.companyId,source.id,source.object_key,source.bytes,source.created_at),
      db.prepare("INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,action,details_json,created_at) VALUES(?,?,?,?,?,'system:migration','legacy.backfilled',?,?) ON CONFLICT(id) DO NOTHING").bind(`attachment-event-legacy-${digest}`,scope.companyId,source.id,linkId,setId,JSON.stringify({ownerType:source.owner_type,ownerId:source.owner_id,purposeKey:mapping.purposeKey}),now),
    ]);report.backfilled++;
  }
  return report;
}

export async function reconcileAttachmentStorage(db:SqlDatabase,files:FileStore,{verifyChecksums=false}:{verifyChecksums?:boolean}={}){
  const rows=(await db.prepare("SELECT id,object_key,preview_object_key,bytes,checksum_sha256,source_state,company_id,legacy_sanitized_only,owner_type,owner_id,purpose,media_type FROM attachments ORDER BY id").all<{id:string;object_key:string;preview_object_key:string|null;bytes:number;checksum_sha256:string;source_state:string;company_id:string|null;legacy_sanitized_only:boolean;owner_type:string;owner_id:string;purpose:string;media_type:string}>()).results;
  const scanRows=(await db.prepare("SELECT DISTINCT ON (attachment_id) attachment_id,attempt,state,result_code,error_code FROM attachment_scan_attempts ORDER BY attachment_id,attempt DESC").all<{attachment_id:string;attempt:number;state:string;result_code:string|null;error_code:string|null}>()).results;
  const latestScan=new Map(scanRows.map(row=>[row.attachment_id,row]));
  const report={sources:rows.length,attachmentDerivatives:0,generatedArtifacts:0,archivedGeneratedArtifacts:0,unknownOwnership:0,legacySanitizedOnly:0,unavailable:0,sealedMalware:0,destroyed:0,
    crossCompanyLinks:[] as string[],crossCompanyIntents:[] as string[],crossCompanyStorage:[] as string[],crossCompanyDerivatives:[] as string[],crossCompanyPublications:[] as string[],
    unknownOwnershipSources:[] as string[],unavailableSources:[] as AttachmentUnavailableSource[],missingObjects:[] as string[],sizeMismatches:[] as string[],checksumMismatches:[] as string[],missingLedger:[] as string[],orphanObjects:[] as string[],ledgerStoredBytes:0,physicalStoredBytes:0,quotaDeltaBytes:0,duplicateLedgerObjects:0};
  report.crossCompanyLinks=(await db.prepare(`SELECT link.id FROM attachment_links link JOIN attachment_sets set_row ON set_row.id=link.set_id
    LEFT JOIN attachments source ON source.id=link.attachment_id
    LEFT JOIN upload_intents intent ON intent.id=link.upload_intent_id
    WHERE (link.attachment_id IS NOT NULL AND (source.id IS NULL OR source.company_id IS DISTINCT FROM set_row.company_id))
       OR (link.upload_intent_id IS NOT NULL AND (intent.id IS NULL OR intent.company_id IS DISTINCT FROM set_row.company_id))
    ORDER BY link.id`).all<{id:string}>()).results.map(row=>row.id);
  report.crossCompanyIntents=(await db.prepare(`SELECT intent.id FROM upload_intents intent
    LEFT JOIN attachment_sets set_row ON set_row.id=intent.attachment_set_id
    LEFT JOIN attachment_links link ON link.id=intent.attachment_link_id
    WHERE intent.attachment_set_id IS NOT NULL AND (
      set_row.id IS NULL OR intent.company_id IS DISTINCT FROM set_row.company_id
      OR intent.attachment_link_id IS NULL OR link.id IS NULL OR link.set_id IS DISTINCT FROM set_row.id)
    ORDER BY intent.id`).all<{id:string}>()).results.map(row=>row.id);
  report.crossCompanyStorage=(await db.prepare(`SELECT ledger.id FROM attachment_storage_ledger ledger
    LEFT JOIN attachments source ON source.id=ledger.attachment_id
    WHERE source.id IS NULL OR (source.company_id IS NOT NULL AND ledger.company_id IS DISTINCT FROM source.company_id) ORDER BY ledger.id`).all<{id:string}>()).results.map(row=>row.id);
  report.crossCompanyDerivatives=(await db.prepare(`SELECT derivative.id FROM attachment_derivatives derivative
    LEFT JOIN attachments source ON source.id=derivative.attachment_id
    WHERE source.id IS NULL OR (source.company_id IS NOT NULL AND derivative.company_id IS DISTINCT FROM source.company_id) ORDER BY derivative.id`).all<{id:string}>()).results.map(row=>row.id);
  report.crossCompanyPublications=(await db.prepare(`SELECT publication.id FROM attachment_publications publication
    LEFT JOIN attachment_links link ON link.id=publication.source_link_id
    LEFT JOIN attachment_sets set_row ON set_row.id=link.set_id
    WHERE set_row.id IS NULL OR publication.company_id IS DISTINCT FROM set_row.company_id ORDER BY publication.id`).all<{id:string}>()).results.map(row=>row.id);
  const known=new Set<string>();
  for(const row of rows){
    if(row.legacy_sanitized_only){report.legacySanitizedOnly++;known.add(row.object_key);const legacy=await files.head(row.object_key);if(!legacy)report.missingObjects.push(`${row.id}:legacy_sanitized_only`);else if(legacy.size!==Number(row.bytes))report.sizeMismatches.push(`${row.id}:legacy_sanitized_only`);if(verifyChecksums&&legacy){const object=await files.get(row.object_key);if(!object||await sha256(await object.arrayBuffer())!==row.checksum_sha256)report.checksumMismatches.push(`${row.id}:legacy_sanitized_only`);}continue;}
    if(!row.company_id){report.unknownOwnership++;report.unknownOwnershipSources.push(row.id);}
    if(["legacy_unscanned","quarantined","scanning","scan_error"].includes(row.source_state)){
      const scan=latestScan.get(row.id);
      report.unavailable++;
      report.unavailableSources.push({id:row.id,companyId:row.company_id,ownerType:row.owner_type,ownerId:row.owner_id,purpose:row.purpose,mediaType:row.media_type,bytes:Number(row.bytes),sourceState:row.source_state,scanAttempt:scan?Number(scan.attempt):null,scanState:scan?.state??null,scanResult:scan?.result_code??null,scanError:scan?.error_code??null});
    }
    if(row.source_state==="malware_rejected")report.sealedMalware++;
    if(row.source_state==="destroyed"){report.destroyed++;continue;}
    known.add(row.object_key);if(row.preview_object_key)known.add(row.preview_object_key);
    const head=await files.head(row.object_key);
    if(!head){report.missingObjects.push(row.id);continue;}
    report.physicalStoredBytes+=head.size;
    if(head.size!==Number(row.bytes))report.sizeMismatches.push(row.id);
    if(verifyChecksums){const object=await files.get(row.object_key);if(!object||await sha256(await object.arrayBuffer())!==row.checksum_sha256)report.checksumMismatches.push(row.id);}
    if(!await db.prepare("SELECT 1 FROM attachment_storage_ledger WHERE attachment_id=? AND variant='original' AND object_key=? AND state='stored'").bind(row.id,row.object_key).first())report.missingLedger.push(row.id);
    if(row.preview_object_key){const preview=await files.head(row.preview_object_key);if(preview)report.physicalStoredBytes+=preview.size;else report.missingObjects.push(`${row.id}:preview`);if(!await db.prepare("SELECT 1 FROM attachment_storage_ledger WHERE attachment_id=? AND variant='preview' AND object_key=? AND state='stored'").bind(row.id,row.preview_object_key).first())report.missingLedger.push(`${row.id}:preview`);}
  }
  const publications=(await db.prepare("SELECT id,derivative_object_key FROM attachment_publications").all<{id:string;derivative_object_key:string}>()).results;for(const publication of publications){known.add(publication.derivative_object_key);const head=await files.head(publication.derivative_object_key);if(head)report.physicalStoredBytes+=head.size;else report.missingObjects.push(`${publication.id}:publication`);if(!await db.prepare("SELECT 1 FROM attachment_storage_ledger WHERE variant='publication' AND object_key=? AND state='stored'").bind(publication.derivative_object_key).first())report.missingLedger.push(`${publication.id}:publication`);}
  const derivatives=(await db.prepare(`SELECT derivative.id,derivative.attachment_id,derivative.object_key,derivative.bytes,derivative.checksum_sha256
    FROM attachment_derivatives derivative JOIN attachments source ON source.id=derivative.attachment_id
    WHERE source.source_state<>'destroyed' ORDER BY derivative.id`).all<{id:string;attachment_id:string;object_key:string;bytes:number;checksum_sha256:string}>()).results;
  report.attachmentDerivatives=derivatives.length;
  for(const derivative of derivatives){
    known.add(derivative.object_key);
    const head=await files.head(derivative.object_key);
    if(!head){report.missingObjects.push(`${derivative.id}:attachment_derivative`);continue;}
    report.physicalStoredBytes+=head.size;
    if(head.size!==Number(derivative.bytes))report.sizeMismatches.push(`${derivative.id}:attachment_derivative`);
    if(verifyChecksums){const object=await files.get(derivative.object_key);if(!object||await sha256(await object.arrayBuffer())!==derivative.checksum_sha256)report.checksumMismatches.push(`${derivative.id}:attachment_derivative`);}
    if(!await db.prepare("SELECT 1 FROM attachment_storage_ledger WHERE attachment_id=? AND variant='derivative' AND object_key=? AND state='stored'").bind(derivative.attachment_id,derivative.object_key).first())report.missingLedger.push(`${derivative.id}:attachment_derivative`);
  }
  const paymentExports=(await db.prepare("SELECT id,file_key,file_checksum FROM hr_payment_batches WHERE file_key IS NOT NULL ORDER BY id").all<{id:string;file_key:string;file_checksum:string|null}>()).results;
  const payrollReviewExports=(await db.prepare("SELECT id,file_key,checksum_sha256 FROM hr_payroll_review_exports ORDER BY id").all<{id:string;file_key:string;checksum_sha256:string}>()).results;
  report.generatedArtifacts=paymentExports.length+payrollReviewExports.length;
  for(const artifact of [
    ...paymentExports.map(row=>({id:row.id,objectKey:row.file_key,checksum:row.file_checksum,kind:"payment_export"})),
    ...payrollReviewExports.map(row=>({id:row.id,objectKey:row.file_key,checksum:row.checksum_sha256,kind:"payroll_review_export"})),
  ]){
    known.add(artifact.objectKey);
    const head=await files.head(artifact.objectKey);
    if(!head){report.missingObjects.push(`${artifact.id}:${artifact.kind}`);continue;}
    if(verifyChecksums&&artifact.checksum){const object=await files.get(artifact.objectKey);if(!object||await sha256(await object.arrayBuffer())!==artifact.checksum)report.checksumMismatches.push(`${artifact.id}:${artifact.kind}`);}
  }
  const intents=(await db.prepare("SELECT staging_key,quarantine_key,final_key FROM upload_intents WHERE status IN ('pending','uploaded')").all<{staging_key:string;quarantine_key:string|null;final_key:string|null}>()).results;for(const intent of intents)for(const key of [intent.staging_key,intent.quarantine_key,intent.final_key])if(key)known.add(key);
  report.ledgerStoredBytes=Number(await db.prepare("SELECT COALESCE(sum(bytes),0) FROM attachment_storage_ledger WHERE state='stored'").first("coalesce"));report.quotaDeltaBytes=report.ledgerStoredBytes-report.physicalStoredBytes;report.duplicateLedgerObjects=Number(await db.prepare("SELECT count(*) FROM (SELECT object_key FROM attachment_storage_ledger WHERE state='stored' GROUP BY object_key HAVING count(*)>1) duplicates").first("count"));
  const paymentExportIds=new Set(paymentExports.map(row=>row.id));
  let cursor:string|undefined;do{const page=await files.list({cursor,limit:1000});for(const object of page.objects){
    if(known.has(object.key))continue;
    const archived=object.key.match(/^[^/]+\/seed-archive\/(.+)-([0-9a-f]{64})\.csv$/);
    if(archived&&paymentExportIds.has(archived[1]!)){
      report.generatedArtifacts++;report.archivedGeneratedArtifacts++;
      if(verifyChecksums){const stored=await files.get(object.key);if(!stored||await sha256(await stored.arrayBuffer())!==archived[2])report.checksumMismatches.push(`${archived[1]}:archived_payment_export`);}
      continue;
    }
    report.orphanObjects.push(object.key);
  }cursor=page.truncated?page.cursor:undefined;}while(cursor);
  return report;
}
