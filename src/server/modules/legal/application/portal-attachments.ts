import type {Clock,IdentifierSource} from "../../shared/application/ports";
import type {PortalScope,PortalAttachment} from "../domain/portal-attachments";
export type PortalMediaType="application/pdf" | "image/png" | "image/jpeg" | "image/webp" | "image/heic" | "image/heif" | "application/vnd.openxmlformats-officedocument.wordprocessingml.document" | "text/csv";
export type PortalAttachmentWrite={owner:"files"};
export type PortalAttachmentRows<Row>={results:Row[]};
export type PortalAttachmentStageScope={sourceId:string;companyId:string;itemId:string;shareId:string;actorId:string;setId:string;linkId:string;uploadId:string;commandId:string;objectKey:string;name:string};
export type PortalAttachmentStagedSource={object_key:string;checksum_sha256:string;media_type:string;bytes:number;expected_target_version:number|null;scope_matches:number};
export type PortalAttachmentRecoverySource=PortalAttachmentStagedSource&{recovery_draft:number;image:string};
export type PortalAttachmentUnit={
existing(facts:PortalAttachmentStageScope):Promise<PortalAttachmentStagedSource|null>;
recoverySource(facts:PortalAttachmentStageScope):Promise<PortalAttachmentRecoverySource|null>;
usedStorage(facts:{companyId:string;}):Promise<number|null>;
pendingStorage(facts:{companyId:string;now:number;}):Promise<number|null>;
storageAllowance(facts:{companyId:string;}):Promise<{ storage_bytes: number; } | null>;
raced(facts:PortalAttachmentStageScope):Promise<PortalAttachmentStagedSource|null>;
draftSet(facts:{setId:string;companyId:string;branchId:string | null;departmentId:string | null;fact4:"legal_item";itemId:string;fact6:"action";commandId:string;PORTAL_PURPOSE:"legal_portal_submission";itemVersion:number;version:number;classification:"public"|"internal"|"confidential"|"restricted";fact12:string;retentionPolicyKey:string | null;requiredCount:number;maxFiles:number;actorId:string;now:number;now2:number;}):PortalAttachmentWrite;
draftLink(facts:{linkId:string;setId:string;actorId:string;now:number;fact4:number;}):PortalAttachmentWrite;
uploadedIntent(facts:{uploadId:string;fact1:"legal_item";itemId:string;PORTAL_PURPOSE:"legal_portal_submission";name:string;mediaType:"application/pdf" | "image/png" | "image/jpeg" | "image/webp" | "image/heic" | "image/heif" | "application/vnd.openxmlformats-officedocument.wordprocessingml.document" | "text/csv";byteLength:number;byteLength2:number;checksum:string;objectKey:string;objectKey2:string;actorId:string;fact12:number;now:number;now2:number;itemVersion:number;setId:string;linkId:string;companyId:string;mediaType2:"application/pdf" | "image/png" | "image/jpeg" | "image/webp" | "image/heic" | "image/heif" | "application/vnd.openxmlformats-officedocument.wordprocessingml.document" | "text/csv";objectKey3:string;}):PortalAttachmentWrite;
quarantinedSource(facts:{sourceId:string;uploadId:string;fact2:"legal_item";itemId:string;PORTAL_PURPOSE:"legal_portal_submission";objectKey:string;name:string;mediaType:"application/pdf" | "image/png" | "image/jpeg" | "image/webp" | "image/heic" | "image/heif" | "application/vnd.openxmlformats-officedocument.wordprocessingml.document" | "text/csv";byteLength:number;checksum:string;actorId:string;now:number;companyId:string;mediaType2:"application/pdf" | "image/png" | "image/jpeg" | "image/webp" | "image/heic" | "image/heif" | "application/vnd.openxmlformats-officedocument.wordprocessingml.document" | "text/csv";}):PortalAttachmentWrite;
linkUpload(facts:{uploadId:string;sourceId:string;linkId:string;}):PortalAttachmentWrite;
pendingScan(facts:{fact0:string;sourceId:string;now:number;}):PortalAttachmentWrite;
storedOriginal(facts:{fact0:string;companyId:string;sourceId:string;objectKey:string;byteLength:number;now:number;}):PortalAttachmentWrite;
quarantineEvent(facts:{fact0:string;companyId:string;sourceId:string;linkId:string;setId:string;actorId:string;fact6:"file.quarantined";fact7:string;now:number;}):PortalAttachmentWrite;
readableSource(facts:{linkId:string;companyId:string;itemId:string;purposeKey:"legal_document";}):Promise<{ link_id: string; attachment_id: string; object_key: string; media_type: string; original_name: string; checksum_sha256: string; source_state: string; } | null>;
allowedRead(facts:{fact0:string;companyId:string;attachment_id:string;link_id:string;actorId:string;now:number;}):Promise<PortalAttachmentRows<Record<string, unknown>>>;
deniedRead(facts:{fact0:string;companyId:string;attachment_id:string;link_id:string;actorId:string;fact5:string;now:number;}):Promise<PortalAttachmentRows<Record<string, unknown>>>;
batch(writes:PortalAttachmentWrite[]):Promise<unknown>;
};
export type PortalAttachmentRepository=PortalAttachmentUnit&{transaction<Result>(work:(unit:PortalAttachmentUnit)=>Promise<Result>):Promise<Result>};
export type PortalAttachmentPolicy={version:number;classification:"public"|"internal"|"confidential"|"restricted";retentionPolicyKey:string|null;requiredCount:number;maxFiles:number};
export type PortalAttachmentStorage={put(key:string,bytes:Uint8Array,mediaType:string,checksum:string):Promise<unknown>;head(key:string):Promise<{customMetadata:Record<string,string>}|null>;get(key:string):Promise<{arrayBuffer():Promise<ArrayBuffer>}|null>};
export function createPortalAttachmentApplication(ports:{repository:PortalAttachmentRepository;storage:PortalAttachmentStorage;clock:Clock;identifiers:IdentifierSource;hash(bytes:Uint8Array):string;reservationMs:number;readOnly():boolean;uploadsFrozen():boolean;readiness():Promise<[{enabled:boolean;ready:boolean},{ready:boolean}]>;policy():PortalAttachmentPolicy;assertMedia(policy:PortalAttachmentPolicy,mediaType:string,bytes:number):void;detectMedia(bytes:Uint8Array,mediaType:string):PortalMediaType|null;scan(id:string):Promise<string>;error(status:number,code:string,message:string):Error;errorCode(error:unknown):string|null}){
const PORTAL_PURPOSE="legal_portal_submission";
async function stagePortalAttachment(
  actorId: string,
  scope: PortalScope,
  name: string,
  declaredMediaType: string,
  bytes: Uint8Array,
  commandId: string,
): Promise<PortalAttachment> {
  if(ports.readOnly())throw ports.error(503,"ATTACHMENTS_READ_ONLY","Attachment intake is disabled during rollback. Existing clean bound evidence remains readable.");
  if(ports.uploadsFrozen())throw ports.error(503,"ATTACHMENT_UPLOAD_FREEZE","File intake is briefly paused for storage reconciliation.");
  const [scanner, preview] = await ports.readiness();
  if (!scanner.enabled || !scanner.ready || !preview.ready)
    throw ports.error(
      503,
      "ATTACHMENT_SCANNER_UNAVAILABLE",
      "File intake is unavailable until the malware scanner, current signatures, and preview tools are ready.",
    );
  const policy = ports.policy();
  ports.assertMedia(policy, declaredMediaType, bytes.byteLength);
  const mediaType = ports.detectMedia(bytes, declaredMediaType);
  if (!mediaType || mediaType !== declaredMediaType)
    throw ports.error(
      422,
      "LEGAL_UPLOAD_TYPE",
      "The file contents do not match the selected type.",
    );

  const checksum = ports.hash(bytes);
  const setId = `attachment-set-legal-portal-${commandId}`;
  const linkId = `attachment-link-legal-portal-${commandId}`;
  const uploadId = `attachment-upload-legal-portal-${commandId}`;
  const sourceId = `attachment-${uploadId}`;
  const objectKey = `legal/${scope.companyId}/${scope.itemId}/quarantine/${uploadId}/original`;
  const stagedScope={sourceId,companyId:scope.companyId,itemId:scope.itemId,shareId:scope.shareId,actorId,setId,linkId,uploadId,commandId,objectKey,name};
  function stagedVersion(existing:PortalAttachmentStagedSource):number {
    if(existing.checksum_sha256!==checksum||existing.media_type!==mediaType||Number(existing.bytes)!==bytes.byteLength||existing.scope_matches!==1||typeof existing.expected_target_version!=="number"||!Number.isSafeInteger(existing.expected_target_version))
      throw ports.error(409,"IDEMPOTENCY_CONFLICT","This upload key was already used for another file.");
    if(existing.expected_target_version!==scope.itemVersion)
      throw ports.error(409,"LEGAL_VERSION_CONFLICT","The legal record changed after this upload was staged. Upload it again with a new key.");
    return existing.expected_target_version;
  }
  const existing = await ports.repository.existing(stagedScope);
  let expectedTargetVersion=existing?stagedVersion(existing):scope.itemVersion;

  if (!existing) {
    const used = Number(
      await ports.repository.usedStorage({companyId:scope.companyId}),
    );
    const pending = Number(
      await ports.repository.pendingStorage({companyId:scope.companyId,now:ports.clock.now()}),
    );
    const limit = Number(
      (
        await ports.repository.storageAllowance({companyId:scope.companyId})
      )?.storage_bytes ?? Number.MAX_SAFE_INTEGER,
    );
    if (used + pending + bytes.byteLength > limit)
      throw ports.error(
        422,
        "STORAGE_QUOTA",
        "Company file storage is full.",
      );

    const stored = await ports.storage.put(objectKey,bytes,mediaType,checksum);
    if (!stored) {
      const prior = await ports.storage.head(objectKey);
      if (prior?.customMetadata.sha256 !== checksum)
        throw ports.error(
          409,
          "UPLOAD_CONFLICT",
          "This upload key already contains different bytes.",
        );
    }

    const now = ports.clock.now();
    expectedTargetVersion=await ports.repository.transaction(async (tx) => {
      const raced = await tx.raced(stagedScope);
      if (raced) {
        return stagedVersion(raced);
      }
      await tx.batch([
        tx.draftSet({setId:setId,companyId:scope.companyId,branchId:scope.branchId,departmentId:scope.departmentId,fact4:"legal_item",itemId:scope.itemId,fact6:"action",commandId:commandId,PORTAL_PURPOSE:PORTAL_PURPOSE,itemVersion:scope.itemVersion,version:policy.version,classification:policy.classification,fact12:JSON.stringify({
              classification: policy.classification,
              portalShareId: scope.shareId,
              policyVersion: policy.version,
            }),retentionPolicyKey:policy.retentionPolicyKey,requiredCount:policy.requiredCount,maxFiles:policy.maxFiles,actorId:actorId,now:now,now2:now}),
        tx.draftLink({linkId:linkId,setId:setId,actorId:actorId,now:now,fact4:now + ports.reservationMs}),
        tx.uploadedIntent({uploadId:uploadId,fact1:"legal_item",itemId:scope.itemId,PORTAL_PURPOSE:PORTAL_PURPOSE,name:name,mediaType:mediaType,byteLength:bytes.byteLength,byteLength2:bytes.byteLength,checksum:checksum,objectKey:objectKey,objectKey2:objectKey,actorId:actorId,fact12:now + ports.reservationMs,now:now,now2:now,itemVersion:scope.itemVersion,setId:setId,linkId:linkId,companyId:scope.companyId,mediaType2:mediaType,objectKey3:objectKey}),
        tx.quarantinedSource({sourceId:sourceId,uploadId:uploadId,fact2:"legal_item",itemId:scope.itemId,PORTAL_PURPOSE:PORTAL_PURPOSE,objectKey:objectKey,name:name,mediaType:mediaType,byteLength:bytes.byteLength,checksum:checksum,actorId:actorId,now:now,companyId:scope.companyId,mediaType2:mediaType}),
        tx.linkUpload({uploadId:uploadId,sourceId:sourceId,linkId:linkId}),
        tx.pendingScan({fact0:`attachment-scan-${ports.identifiers.next()}`,sourceId:sourceId,now:now}),
        tx.storedOriginal({fact0:`attachment-storage-${ports.identifiers.next()}`,companyId:scope.companyId,sourceId:sourceId,objectKey:objectKey,byteLength:bytes.byteLength,now:now}),
        tx.quarantineEvent({fact0:`attachment-event-${ports.identifiers.next()}`,companyId:scope.companyId,sourceId:sourceId,linkId:linkId,setId:setId,actorId:actorId,fact6:"file.quarantined",fact7:JSON.stringify({
              checksum,
              bytes: bytes.byteLength,
              detectedMediaType: mediaType,
              portalShareId: scope.shareId,
            }),now:now}),
      ]);
      return scope.itemVersion;
    });
  }

  const state = await ports.scan(sourceId);
  if (state !== "available")
    throw ports.error(
      state === "malware_rejected" ? 422 : 503,
      state === "malware_rejected"
        ? "LEGAL_UPLOAD_REJECTED"
        : "LEGAL_SCAN_UNAVAILABLE",
      state === "malware_rejected"
        ? "The upload was rejected by malware scanning."
        : "The upload could not pass malware scanning.",
    );
  return {
    setId,
    linkId,
    sourceId,
    objectKey,
    checksum,
    mediaType,
    bytes: bytes.byteLength,
    expectedTargetVersion,
  };
}

async function portalAttachmentBytes(
  input: {
    companyId: string;
    itemId: string;
    linkId: string;
    actorId: string;
    purposeKey: "legal_document";
  },
) {
  const row = await ports.repository.readableSource({linkId:input.linkId,companyId:input.companyId,itemId:input.itemId,purposeKey:input.purposeKey});
  if (!row)
    throw ports.error(
      404,
      "ATTACHMENT_LINK_NOT_FOUND",
      "File link not found.",
    );
  try {
    const object = await ports.storage.get(row.object_key);
    if (!object)
      throw ports.error(
        503,
        "FILE_UNAVAILABLE",
        "The private file is temporarily unavailable.",
      );
    const bytes = new Uint8Array(await object.arrayBuffer());
    if (ports.hash(bytes) !== row.checksum_sha256)
      throw ports.error(
        409,
        "LEGAL_INTEGRITY",
        "Document integrity check failed.",
      );
    await ports.repository.allowedRead({fact0:`attachment-access-${ports.identifiers.next()}`,companyId:input.companyId,attachment_id:row.attachment_id,link_id:row.link_id,actorId:input.actorId,now:ports.clock.now()});
    return { bytes, mediaType: row.media_type, filename: row.original_name };
  } catch (error) {
    await ports.repository.deniedRead({fact0:`attachment-access-${ports.identifiers.next()}`,companyId:input.companyId,attachment_id:row.attachment_id,link_id:row.link_id,actorId:input.actorId,fact5:ports.errorCode(error)??"ACCESS_DENIED",now:ports.clock.now()})
      .catch(() => {});
    throw error;
  }
}

return {stagePortalAttachment,portalAttachmentBytes};}
