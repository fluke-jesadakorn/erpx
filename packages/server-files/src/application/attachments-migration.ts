import type {LegacySource,Scope,SpecializedSource,AttachmentUnavailableSource} from '@erpx/server-files/domain/attachments/migration';import type {AttachmentPolicy} from '@erpx/server-files/domain/attachments/types';import type {legacyAttachmentPolicy} from '@erpx/server-files/domain/attachments/legacy-policy';import type {attachmentRetentionSnapshot,attachmentPolicy} from '@erpx/server-files/public/attachments/policy';import type {AttachmentReadStore} from '@erpx/server-files/domain/attachment-read-store';import type {FileObjectMetadata as FileMetadata} from '@erpx/server-files/domain/file-object';import type {verifiedPreservedDirectory} from '@erpx/server-files/public/attachments/orphan-preservation';import type {Clock} from '@erpx/server-shared/index';type MigrationFileStore=AttachmentReadStore;export interface AttachmentMigrationRepository<Effect>{readBackfillSpecializedAttachmentsMessaging():Promise<{results:(Record<string,unknown>)[]}>;
readBackfillSpecializedAttachmentsDocuments():Promise<{results:(Record<string,unknown>)[]}>;
readBackfillSpecializedAttachmentsExternal():Promise<{results:(Record<string,unknown>)[]}>;
readBackfillSpecializedAttachmentsImages():Promise<{results:(Record<string,unknown>)[]}>;
stageBackfillSpecializedAttachmentsEffect(setId:string,companyId:string,branchId:string | null,departmentId:string | null,targetType:"messaging_room" | "legal_item" | "product",targetId:string,componentKind:"record" | "action" | "line" | "event" | "message" | "publication",componentRef:string,purposeKey:string,targetVersion:number,version:number,classification:import("@erpx/server-files/domain/attachments/types").AttachmentClassification,value:string,key:string | null,version2:number | null,retainUntil:null,destructionMode:"review" | null,requiredCount:number,maxFiles:number,createdBy:string,createdAt:number,now:number):Effect;
stageBackfillSpecializedAttachmentsEffect2(linkId:string,setId:string,createdBy:string,createdAt:number,expiresAt:number):Effect;
stageBackfillSpecializedAttachmentsEffect3(uploadId:string,targetType:"messaging_room" | "legal_item" | "product",targetId:string,purposeKey:string,originalName:string,mediaType:string,bytes:number,bytes2:number,checksum:string,objectKey:string,objectKey2:string,revision:number,createdBy:string,expiresAt:number,createdAt:number,now:number,targetVersion:number,setId:string,linkId:string,companyId:string,objectKey3:string):Effect;
stageBackfillSpecializedAttachmentsEffect4(uploadId:string,linkId:string):Effect;
stageBackfillSpecializedAttachmentsEffect5(attachmentId:string,uploadId:string,targetType:"messaging_room" | "legal_item" | "product",targetId:string,purposeKey:string,objectKey:string,revision:number,originalName:string,mediaType:string,bytes:number,checksum:string,createdBy:string,value:0 | 1,createdAt:number,companyId:string):Effect;
stageBackfillSpecializedAttachmentsEffect6(attachmentId:string,linkId:string):Effect;
stageBackfillSpecializedAttachmentsEffect7(scanId:string,attachmentId:string,now:number):Effect;
stageBackfillSpecializedAttachmentsEffect8(value:string,companyId:string,attachmentId:string,objectKey:string,bytes:number,createdAt:number):Effect;
stageBackfillSpecializedAttachmentsEffect9(value:string,companyId:string,attachmentId:string,linkId:string,setId:string,value2:string,now:number):Effect;
stageBackfillSpecializedAttachmentsEffect10(setId:string,linkId:string,attachmentId:string,id:string):Effect;
stageBackfillSpecializedAttachmentsEffect11(linkId:string,attachmentId:string,id:string):Effect;
stageBackfillSpecializedAttachmentsEffect12(linkId:string,attachmentId:string,id:string):Effect;
stageBackfillSpecializedAttachmentsEffect13(setId:string,linkId:string,attachmentId:string,checksum:string,id:string):Effect;
stageBackfillSpecializedAttachmentsEffect14(value:string,attachmentId:string,companyId:string,objectKey:string,checksum:string,checksum2:string,bytes:number,createdBy:string,createdAt:number):Effect;
stageBackfillSpecializedAttachmentsEffect15(value:string,companyId:string,attachmentId:string,objectKey:string,bytes:number,createdAt:number):Effect;
stageBackfillSpecializedAttachmentsEffect16(boundAt:number | null,value:string,id:string,linkId:string):Effect;
stageBackfillSpecializedAttachmentsEffect17(boundAt:number | null,value:string,now:number,setId:string):Effect;
readBackfillLegacyAttachmentsSources():Promise<{results:(LegacySource)[]}>;
stageBackfillLegacyAttachmentsEffect(id:string):Promise<{success:boolean;meta?:{changes?:number;duration?:number;last_row_id?:number|string}}>;
stageBackfillLegacyAttachmentsEffect2(id:string):Promise<{success:boolean;meta?:{changes?:number;duration?:number;last_row_id?:number|string}}>;
stageBackfillLegacyAttachmentsEffect3(id:string):Promise<{success:boolean;meta?:{changes?:number;duration?:number;last_row_id?:number|string}}>;
stageBackfillLegacyAttachmentsEffect4(id:string):Promise<{success:boolean;meta?:{changes?:number;duration?:number;last_row_id?:number|string}}>;
stageBackfillLegacyAttachmentsEffect5(setId:string,companyId:string,branchId:string | null,departmentId:string | null,owner_type:string,owner_id:string,componentKind:"record" | "action" | "line" | "event" | "message" | "publication",componentRef:string | null,purposeKey:string,version:number,version2:number,classification:import("@erpx/server-files/domain/attachments/types").AttachmentClassification,value:string,key:string | null,version3:number | null,retainUntil:null,destructionMode:"review" | null,requiredCount:number,maxFiles:number,created_by:string,created_at:number,now:number,now2:number,value2:string):Effect;
stageBackfillLegacyAttachmentsEffect6(linkId:string,setId:string,id:string,upload_id:string,created_by:string,created_at:number,value:number,now:number,owner_id:string):Effect;
stageBackfillLegacyAttachmentsEffect7(companyId:string,id:string):Effect;
stageBackfillLegacyAttachmentsEffect8(setId:string,linkId:string,companyId:string,object_key:string,now:number,upload_id:string):Effect;
stageBackfillLegacyAttachmentsEffect9(value:string,id:string,now:number):Effect;
stageBackfillLegacyAttachmentsEffect10(value:string,companyId:string,id:string,object_key:string,bytes:number,created_at:number):Effect;
stageBackfillLegacyAttachmentsEffect11(value:string,companyId:string,id:string,linkId:string,setId:string,value2:string,now:number):Effect;
readReconcileAttachmentStorageRows():Promise<{results:({id:string;object_key:string;preview_object_key:string|null;preview_bytes:number|null;preview_checksum_sha256:string|null;bytes:number;checksum_sha256:string;source_state:string;company_id:string|null;legacy_sanitized_only:boolean;owner_type:string;owner_id:string;purpose:string;media_type:string})[]}>;
readReconcileAttachmentStorageScanRows():Promise<{results:({attachment_id:string;attempt:number;state:string;result_code:string|null;error_code:string|null})[]}>;
readReconcileAttachmentStorageEffect():Promise<{results:({id:string})[]}>;
readReconcileAttachmentStorageEffect2():Promise<{results:({id:string})[]}>;
readReconcileAttachmentStorageEffect3():Promise<{results:({id:string})[]}>;
readReconcileAttachmentStorageEffect4():Promise<{results:({id:string})[]}>;
readReconcileAttachmentStorageEffect5():Promise<{results:({id:string})[]}>;
readReconcileAttachmentStorageStoredLedgerMatches(companyId:string | null,attachmentId:string | null,variant:string,objectKey:string,bytes:number):Promise<Record<string,unknown>|null>;
readReconcileAttachmentStorageEffect6():Promise<unknown>;
readReconcileAttachmentStoragePreserved():Promise<{results:({
        id:string;object_key:string;object_sha256:string;manifest_sha256:string;bytes:number;preserved_directory:string;preservation_mode:string;
      })[]}>;
readReconcileAttachmentStoragePublications():Promise<{results:({id:string;company_id:string;derivative_object_key:string;derivative_bytes:number;derivative_checksum_sha256:string;attachment_id:string|null})[]}>;
readReconcileAttachmentStorageDerivatives():Promise<{results:({id:string;attachment_id:string;company_id:string;object_key:string;bytes:number;checksum_sha256:string})[]}>;
readReconcileAttachmentStoragePaymentExports():Promise<{results:({id:string;file_key:string;file_checksum:string|null})[]}>;
readReconcileAttachmentStoragePayrollReviewExports():Promise<{results:({id:string;file_key:string;checksum_sha256:string})[]}>;
readReconcileAttachmentStorageIntents():Promise<{results:({staging_key:string;quarantine_key:string|null;final_key:string|null})[]}>;
readReconcileAttachmentStorageEffect7():Promise<unknown>;
readReconcileAttachmentStorageEffect8():Promise<unknown>;}export interface AttachmentMigrationPorts<Effect>{repository:AttachmentMigrationRepository<Effect>;clock:Clock;policy(...args:Parameters<typeof attachmentPolicy>):AttachmentPolicy;legacyPolicy:typeof legacyAttachmentPolicy;retention:typeof attachmentRetentionSnapshot;ownerScope(source:LegacySource):Promise<Scope|null>;brandCompanies():Promise<{company_id:string}[]>;digest(value:string):string;bytesDigest(value:ArrayBufferLike):Promise<string>;thumbnailChecksum(bytes:Uint8Array):Promise<string>;preservationMetadataDigest(head:FileMetadata):string;verifiedPreservedDirectory:typeof verifiedPreservedDirectory;batch(effects:Effect[]):Promise<unknown>;}export function createAttachmentMigrationApplication<Effect>(ports:AttachmentMigrationPorts<Effect>){const r=ports.repository;async function legacyScope(source:LegacySource):Promise<Scope|null>{if(source.owner_type!=="holding_brand")return ports.ownerScope(source);const companies=await ports.brandCompanies();return companies.length===1?{companyId:companies[0]!.company_id,branchId:null,departmentId:null,version:1}:null;}async function backfillSpecializedAttachments({apply=false,files}:{apply?:boolean;files?:MigrationFileStore}={}){
  const messaging=(await r.readBackfillSpecializedAttachmentsMessaging()).results.map(row=>({kind:"messaging" as const,id:String(row.id),companyId:String(row.company_id),branchId:null,departmentId:null,targetType:"messaging_room" as const,targetId:String(row.target_id),targetVersion:Number(row.target_version),componentKind:"message" as const,componentRef:String(row.component_ref),purposeKey:"message_attachment",originalName:String(row.original_name),mediaType:String(row.media_type),bytes:Number(row.bytes),checksum:String(row.checksum),objectKey:String(row.object_key),createdBy:String(row.created_by),createdAt:Number(row.created_at),revision:1,bound:Boolean(row.bound),legalHold:false}));
  const documents=(await r.readBackfillSpecializedAttachmentsDocuments()).results.map(row=>({kind:"legal_document" as const,id:String(row.id),companyId:String(row.company_id),branchId:row.branch_id?String(row.branch_id):null,departmentId:row.department_id?String(row.department_id):null,targetType:"legal_item" as const,targetId:String(row.target_id),targetVersion:Number(row.target_version),componentKind:"record" as const,componentRef:String(row.component_ref),purposeKey:"legal_document",originalName:String(row.original_name),mediaType:String(row.media_type),bytes:Number(row.bytes),checksum:String(row.checksum),objectKey:String(row.object_key),createdBy:String(row.created_by),createdAt:Number(row.created_at),revision:Number(row.revision),bound:true,legalHold:Boolean(row.legal_hold)}));
  const external=(await r.readBackfillSpecializedAttachmentsExternal()).results.map(row=>({kind:"legal_external" as const,id:String(row.id),companyId:String(row.company_id),branchId:row.branch_id?String(row.branch_id):null,departmentId:row.department_id?String(row.department_id):null,targetType:"legal_item" as const,targetId:String(row.target_id),targetVersion:Number(row.target_version),componentKind:"action" as const,componentRef:String(row.component_ref),purposeKey:"legal_portal_submission",originalName:String(row.original_name),mediaType:String(row.media_type),bytes:Number(row.bytes),checksum:String(row.checksum),objectKey:String(row.object_key),createdBy:String(row.created_by),createdAt:Number(row.created_at),revision:1,bound:true,legalHold:Boolean(row.legal_hold)}));
  const sources:SpecializedSource[]=[...messaging,...documents,...external],report={examined:sources.length,ready:0,backfilled:0,unknown:[] as Array<{kind:string;id:string;reason:string}>};
  const images=(await r.readBackfillSpecializedAttachmentsImages()).results;
  report.examined+=images.length;
  for(const image of images){
    const id=String(image.id),unknown=(reason:string)=>report.unknown.push({kind:"pos_product_image",id,reason});
    if(!files){unknown("FILE_STORE_REQUIRED");continue;}
    const original=await files.get(String(image.object_key)),thumbnail=await files.get(String(image.thumbnail_key));
    if(!original||!thumbnail){unknown("POS_IMAGE_OBJECT_MISSING");continue;}
    const originalBytes=new Uint8Array(await original.arrayBuffer()),thumbnailBytes=new Uint8Array(await thumbnail.arrayBuffer());
    const checksum=await ports.bytesDigest(originalBytes.buffer),thumbnailChecksum=await ports.bytesDigest(thumbnailBytes.buffer);
    if(checksum!==image.sha256){unknown("POS_IMAGE_CHECKSUM_MISMATCH");continue;}
    try{
      // Recognize only the exact legacy transformation, never an arbitrary file
      // which happens to share the thumbnail namespace or a Company prefix.
      const expectedChecksum=await ports.thumbnailChecksum(originalBytes);
      if(expectedChecksum!==thumbnailChecksum){unknown("POS_THUMBNAIL_PROVENANCE_MISMATCH");continue;}
    }catch{unknown("POS_IMAGE_FORMAT_INVALID");continue;}
    sources.push({kind:"pos_product_image",id,companyId:String(image.company_id),branchId:null,departmentId:null,targetType:"product",
      targetId:String(image.product_id),targetVersion:Number(image.product_version),componentKind:"record",componentRef:id,purposeKey:"product_image",
      originalName:`${id}.${image.mime_type==="image/png"?"png":image.mime_type==="image/webp"?"webp":"jpg"}`,mediaType:String(image.mime_type),bytes:originalBytes.byteLength,
      checksum,objectKey:String(image.object_key),createdBy:String(image.created_by),createdAt:Number(image.created_at),revision:1,bound:true,legalHold:false,
      thumbnail:{objectKey:String(image.thumbnail_key),bytes:thumbnailBytes.byteLength,checksum:thumbnailChecksum}});
  }
  for(const source of sources){
    let policy;try{policy=ports.policy(source.targetType,source.purposeKey,source.componentKind);}catch{report.unknown.push({kind:source.kind,id:source.id,reason:"POLICY_UNKNOWN"});continue;}
    if(!policy.mediaTypes.includes(source.mediaType)||source.bytes<1||source.bytes>policy.maxBytes){report.unknown.push({kind:source.kind,id:source.id,reason:"FORMAT_OUTSIDE_POLICY"});continue;}
    report.ready++;if(!apply)continue;
    const digest=ports.digest(`${source.kind}\0${source.id}`).slice(0,32),setId=`attachment-set-specialized-${digest}`,linkId=`attachment-link-specialized-${digest}`,uploadId=`attachment-upload-specialized-${digest}`,attachmentId=`attachment-specialized-${digest}`,scanId=`attachment-scan-specialized-${digest}`,now=ports.clock.now(),retention=ports.retention(policy),expiresAt=source.createdAt+7*24*60*60_000,boundAt=source.bound?source.createdAt:null;
    await ports.batch([
      r.stageBackfillSpecializedAttachmentsEffect(setId,source.companyId,source.branchId,source.departmentId,source.targetType,source.targetId,source.componentKind,source.componentRef,source.purposeKey,source.targetVersion,policy.version,policy.classification,JSON.stringify({classification:policy.classification,policyVersion:policy.version,legacySpecialized:true}),retention.key,retention.version,retention.retainUntil,retention.destructionMode,policy.requiredCount,policy.maxFiles,source.createdBy,source.createdAt,now),
      r.stageBackfillSpecializedAttachmentsEffect2(linkId,setId,source.createdBy,source.createdAt,expiresAt),
      r.stageBackfillSpecializedAttachmentsEffect3(uploadId,source.targetType,source.targetId,source.purposeKey,source.originalName,source.mediaType,source.bytes,source.bytes,source.checksum,source.objectKey,source.objectKey,source.revision,source.createdBy,expiresAt,source.createdAt,now,source.targetVersion,setId,linkId,source.companyId,source.objectKey),
      r.stageBackfillSpecializedAttachmentsEffect4(uploadId,linkId),
      r.stageBackfillSpecializedAttachmentsEffect5(attachmentId,uploadId,source.targetType,source.targetId,source.purposeKey,source.objectKey,source.revision,source.originalName,source.mediaType,source.bytes,source.checksum,source.createdBy,source.legalHold?1:0,source.createdAt,source.companyId),
      r.stageBackfillSpecializedAttachmentsEffect6(attachmentId,linkId),
      r.stageBackfillSpecializedAttachmentsEffect7(scanId,attachmentId,now),
      r.stageBackfillSpecializedAttachmentsEffect8(`attachment-storage-specialized-${digest}`,source.companyId,attachmentId,source.objectKey,source.bytes,source.createdAt),
      r.stageBackfillSpecializedAttachmentsEffect9(`attachment-event-specialized-${digest}`,source.companyId,attachmentId,linkId,setId,JSON.stringify({kind:source.kind,legacyId:source.id}),now),
      source.kind==="messaging"
        ?r.stageBackfillSpecializedAttachmentsEffect10(setId,linkId,attachmentId,source.id)
        :source.kind==="legal_document"
          ?r.stageBackfillSpecializedAttachmentsEffect11(linkId,attachmentId,source.id)
          :source.kind==="legal_external"
            ?r.stageBackfillSpecializedAttachmentsEffect12(linkId,attachmentId,source.id)
            :r.stageBackfillSpecializedAttachmentsEffect13(setId,linkId,attachmentId,source.thumbnail!.checksum,source.id),
      ...(source.thumbnail?[
        r.stageBackfillSpecializedAttachmentsEffect14(`attachment-thumbnail-${digest}`,attachmentId,source.companyId,source.thumbnail.objectKey,source.thumbnail.checksum,source.checksum,source.thumbnail.bytes,source.createdBy,source.createdAt),
        r.stageBackfillSpecializedAttachmentsEffect15(`attachment-storage-thumbnail-${digest}`,source.companyId,attachmentId,source.thumbnail.objectKey,source.thumbnail.bytes,source.createdAt),
      ]:[]),
      ...(source.bound?[
        r.stageBackfillSpecializedAttachmentsEffect16(boundAt,`legacy_${source.kind}`,source.id,linkId),
        r.stageBackfillSpecializedAttachmentsEffect17(boundAt,`legacy-specialized-${source.id}`,now,setId),
      ]:[]),
    ]);report.backfilled++;
  }
  return report;
}
async function backfillLegacyAttachments({apply=false}:{apply?:boolean}={}){
  const sources=(await r.readBackfillLegacyAttachmentsSources()).results;
  const report={examined:sources.length,ready:0,legacySanitizedOnly:[] as string[],unknown:[] as Array<{id:string;ownerType:string;ownerId:string;reason:string;targetCompanyId?:string}>,backfilled:0};
  for(const source of sources){
    const mapping=ports.legacyPolicy(source.owner_type,source.purpose),scope=await legacyScope(source);
    const unknown=(reason:string)=>({id:source.id,ownerType:source.owner_type,ownerId:source.owner_id,reason,...(scope?{targetCompanyId:scope.companyId}:{})});
    if(mapping&&source.owner_type==="holding_brand"&&!scope){report.legacySanitizedOnly.push(source.id);if(apply)await r.stageBackfillLegacyAttachmentsEffect(source.id);continue;}
    if(!mapping||!scope){report.unknown.push(unknown(!mapping?"POLICY_UNKNOWN":"OWNER_UNKNOWN"));if(apply)await r.stageBackfillLegacyAttachmentsEffect2(source.id);continue;}
    let policy;try{policy=ports.policy(source.owner_type,mapping.purposeKey,mapping.componentKind);}catch{report.unknown.push(unknown("POLICY_UNKNOWN"));if(apply)await r.stageBackfillLegacyAttachmentsEffect3(source.id);continue;}
    const retention=ports.retention(policy);
    if(!policy.mediaTypes.includes(source.media_type)||source.bytes<1||source.bytes>policy.maxBytes){report.unknown.push(unknown("FORMAT_OUTSIDE_POLICY"));if(apply)await r.stageBackfillLegacyAttachmentsEffect4(source.id);continue;}
    report.ready++;if(!apply)continue;
    const digest=ports.digest(source.id).slice(0,32),setId=`attachment-set-legacy-${digest}`,linkId=`attachment-link-legacy-${digest}`,now=ports.clock.now();
    await ports.batch([
      r.stageBackfillLegacyAttachmentsEffect5(setId,scope.companyId,scope.branchId,scope.departmentId,source.owner_type,source.owner_id,mapping.componentKind,mapping.componentRef,mapping.purposeKey,scope.version,policy.version,policy.classification,JSON.stringify({classification:policy.classification,policyVersion:policy.version,legacy:true}),retention.key,retention.version,retention.retainUntil,retention.destructionMode,policy.requiredCount,policy.maxFiles,source.created_by,source.created_at,now,now,`legacy-import-${source.id}`),
      r.stageBackfillLegacyAttachmentsEffect6(linkId,setId,source.id,source.upload_id,source.created_by,source.created_at,source.created_at+7*24*60*60_000,now,source.owner_id),
      r.stageBackfillLegacyAttachmentsEffect7(scope.companyId,source.id),
      r.stageBackfillLegacyAttachmentsEffect8(setId,linkId,scope.companyId,source.object_key,now,source.upload_id),
      r.stageBackfillLegacyAttachmentsEffect9(`attachment-scan-legacy-${digest}`,source.id,now),
      r.stageBackfillLegacyAttachmentsEffect10(`attachment-storage-legacy-${digest}`,scope.companyId,source.id,source.object_key,source.bytes,source.created_at),
      r.stageBackfillLegacyAttachmentsEffect11(`attachment-event-legacy-${digest}`,scope.companyId,source.id,linkId,setId,JSON.stringify({ownerType:source.owner_type,ownerId:source.owner_id,purposeKey:mapping.purposeKey}),now),
    ]);report.backfilled++;
  }
  return report;
}
async function reconcileAttachmentStorage(files:MigrationFileStore,{verifyChecksums=false}:{verifyChecksums?:boolean}={}){
  const rows=(await r.readReconcileAttachmentStorageRows()).results;
  const scanRows=(await r.readReconcileAttachmentStorageScanRows()).results;
  const latestScan=new Map(scanRows.map(row=>[row.attachment_id,row]));
  const report={sources:rows.length,attachmentDerivatives:0,generatedArtifacts:0,archivedGeneratedArtifacts:0,preservedPendingOwner:0,preservedUnownedBytes:0,unknownOwnership:0,legacySanitizedOnly:0,unavailable:0,sealedMalware:0,destroyed:0,
    crossCompanyLinks:[] as string[],crossCompanyIntents:[] as string[],crossCompanyStorage:[] as string[],crossCompanyDerivatives:[] as string[],crossCompanyPublications:[] as string[],
    unknownOwnershipSources:[] as string[],unavailableSources:[] as AttachmentUnavailableSource[],missingObjects:[] as string[],sizeMismatches:[] as string[],checksumMismatches:[] as string[],missingLedger:[] as string[],orphanObjects:[] as string[],ledgerStoredBytes:0,physicalStoredBytes:0,quotaDeltaBytes:0,duplicateLedgerObjects:0};
  report.crossCompanyLinks=(await r.readReconcileAttachmentStorageEffect()).results.map(row=>row.id);
  report.crossCompanyIntents=(await r.readReconcileAttachmentStorageEffect2()).results.map(row=>row.id);
  report.crossCompanyStorage=(await r.readReconcileAttachmentStorageEffect3()).results.map(row=>row.id);
  report.crossCompanyDerivatives=(await r.readReconcileAttachmentStorageEffect4()).results.map(row=>row.id);
  report.crossCompanyPublications=(await r.readReconcileAttachmentStorageEffect5()).results.map(row=>row.id);
  const known=new Set<string>();
  const storedLedgerMatches=async(companyId:string|null,attachmentId:string|null,variant:string,objectKey:string,bytes:number)=>Boolean(await r.readReconcileAttachmentStorageStoredLedgerMatches(companyId,attachmentId,variant,objectKey,bytes));
  if(await r.readReconcileAttachmentStorageEffect6()) {
    const preserved=(await r.readReconcileAttachmentStoragePreserved()).results;
    const {preservationMetadataDigest,verifiedPreservedDirectory}=ports;
    for(const row of preserved) {
      report.preservedPendingOwner++;report.preservedUnownedBytes+=Number(row.bytes);
      if(row.preservation_mode==='file-store-sealed') {
        known.add(row.object_key);const head=await files.head(row.object_key);
        if(!head)report.missingObjects.push(`${row.id}:preserved_unowned`);
        else {
          if(head.size!==Number(row.bytes))report.sizeMismatches.push(`${row.id}:preserved_unowned`);
          if(head.etag!==row.object_sha256||preservationMetadataDigest(head)!==row.manifest_sha256)report.checksumMismatches.push(`${row.id}:preserved_unowned_manifest`);
          if(verifyChecksums){const body=await files.get(row.object_key);if(!body||await ports.bytesDigest(await body.arrayBuffer())!==row.object_sha256)report.checksumMismatches.push(`${row.id}:preserved_unowned_bytes`);}
        }
      }else {
        try {const proof=await verifiedPreservedDirectory(row.preserved_directory,row.object_key);
          if(proof.bytes!==Number(row.bytes)||proof.objectSha256!==row.object_sha256||proof.manifestSha256!==row.manifest_sha256)report.checksumMismatches.push(`${row.id}:preserved_directory`);
        }catch{report.missingObjects.push(`${row.id}:preserved_directory`);}
        if(await files.head(row.object_key))report.checksumMismatches.push(`${row.id}:preserved_key_reappeared`);
      }
    }
  }
  for(const row of rows){
    if(row.legacy_sanitized_only){report.legacySanitizedOnly++;known.add(row.object_key);const legacy=await files.head(row.object_key);if(!legacy)report.missingObjects.push(`${row.id}:legacy_sanitized_only`);else if(legacy.size!==Number(row.bytes))report.sizeMismatches.push(`${row.id}:legacy_sanitized_only`);if(verifyChecksums&&legacy){const object=await files.get(row.object_key);if(!object||await ports.bytesDigest(await object.arrayBuffer())!==row.checksum_sha256)report.checksumMismatches.push(`${row.id}:legacy_sanitized_only`);}continue;}
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
    if(verifyChecksums){const object=await files.get(row.object_key);if(!object||await ports.bytesDigest(await object.arrayBuffer())!==row.checksum_sha256)report.checksumMismatches.push(row.id);}
    if(!await storedLedgerMatches(row.company_id,row.id,"original",row.object_key,Number(row.bytes)))report.missingLedger.push(row.id);
    if(row.preview_object_key){
      const marker=`${row.id}:preview`,preview=await files.head(row.preview_object_key);
      if(preview){
        report.physicalStoredBytes+=preview.size;
        if(preview.size!==Number(row.preview_bytes))report.sizeMismatches.push(marker);
        if(verifyChecksums){const object=await files.get(row.preview_object_key);if(!object||await ports.bytesDigest(await object.arrayBuffer())!==row.preview_checksum_sha256)report.checksumMismatches.push(marker);}
      }else report.missingObjects.push(marker);
      if(!await storedLedgerMatches(row.company_id,row.id,"preview",row.preview_object_key,Number(row.preview_bytes)))report.missingLedger.push(marker);
    }
  }
  const publications=(await r.readReconcileAttachmentStoragePublications()).results;
  for(const publication of publications){
    const marker=`${publication.id}:publication`;
    known.add(publication.derivative_object_key);const head=await files.head(publication.derivative_object_key);
    if(head){
      report.physicalStoredBytes+=head.size;
      if(head.size!==Number(publication.derivative_bytes))report.sizeMismatches.push(marker);
      if(verifyChecksums){const object=await files.get(publication.derivative_object_key);if(!object||await ports.bytesDigest(await object.arrayBuffer())!==publication.derivative_checksum_sha256)report.checksumMismatches.push(marker);}
    }else report.missingObjects.push(marker);
    if(!await storedLedgerMatches(publication.company_id,publication.attachment_id,"publication",publication.derivative_object_key,Number(publication.derivative_bytes)))report.missingLedger.push(marker);
  }
  const derivatives=(await r.readReconcileAttachmentStorageDerivatives()).results;
  report.attachmentDerivatives=derivatives.length;
  for(const derivative of derivatives){
    known.add(derivative.object_key);
    const head=await files.head(derivative.object_key);
    if(!head){report.missingObjects.push(`${derivative.id}:attachment_derivative`);continue;}
    report.physicalStoredBytes+=head.size;
    if(head.size!==Number(derivative.bytes))report.sizeMismatches.push(`${derivative.id}:attachment_derivative`);
    if(verifyChecksums){const object=await files.get(derivative.object_key);if(!object||await ports.bytesDigest(await object.arrayBuffer())!==derivative.checksum_sha256)report.checksumMismatches.push(`${derivative.id}:attachment_derivative`);}
    if(!await storedLedgerMatches(derivative.company_id,derivative.attachment_id,"derivative",derivative.object_key,Number(derivative.bytes)))report.missingLedger.push(`${derivative.id}:attachment_derivative`);
  }
  const paymentExports=(await r.readReconcileAttachmentStoragePaymentExports()).results;
  const payrollReviewExports=(await r.readReconcileAttachmentStoragePayrollReviewExports()).results;
  report.generatedArtifacts=paymentExports.length+payrollReviewExports.length;
  for(const artifact of [
    ...paymentExports.map(row=>({id:row.id,objectKey:row.file_key,checksum:row.file_checksum,kind:"payment_export"})),
    ...payrollReviewExports.map(row=>({id:row.id,objectKey:row.file_key,checksum:row.checksum_sha256,kind:"payroll_review_export"})),
  ]){
    known.add(artifact.objectKey);
    const head=await files.head(artifact.objectKey);
    if(!head){report.missingObjects.push(`${artifact.id}:${artifact.kind}`);continue;}
    if(verifyChecksums&&artifact.checksum){const object=await files.get(artifact.objectKey);if(!object||await ports.bytesDigest(await object.arrayBuffer())!==artifact.checksum)report.checksumMismatches.push(`${artifact.id}:${artifact.kind}`);}
  }
  const intents=(await r.readReconcileAttachmentStorageIntents()).results;for(const intent of intents)for(const key of [intent.staging_key,intent.quarantine_key,intent.final_key])if(key)known.add(key);
  report.ledgerStoredBytes=Number(await r.readReconcileAttachmentStorageEffect7());report.quotaDeltaBytes=report.ledgerStoredBytes-report.physicalStoredBytes;report.duplicateLedgerObjects=Number(await r.readReconcileAttachmentStorageEffect8());
  const paymentExportIds=new Set(paymentExports.map(row=>row.id));
  let cursor:string|undefined;do{const page=await files.list({cursor,limit:1000});for(const object of page.objects){
    if(known.has(object.key))continue;
    const archived=object.key.match(/^[^/]+\/seed-archive\/(.+)-([0-9a-f]{64})\.csv$/);
    if(archived&&paymentExportIds.has(archived[1]!)){
      report.generatedArtifacts++;report.archivedGeneratedArtifacts++;
      if(verifyChecksums){const stored=await files.get(object.key);if(!stored||await ports.bytesDigest(await stored.arrayBuffer())!==archived[2])report.checksumMismatches.push(`${archived[1]}:archived_payment_export`);}
      continue;
    }
    report.orphanObjects.push(object.key);
  }cursor=page.truncated?page.cursor:undefined;}while(cursor);
  return report;
}return {backfillSpecializedAttachments,backfillLegacyAttachments,reconcileAttachmentStorage};}
