import {z} from "zod";
import type {Clock,IdentifierSource} from "@erpx/server-shared/application/ports";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {executedCopySchema,type ExecutedCopyInput,type DocumentVersion,type LegalItem} from "@erpx/server-legal/domain/model";
import type {LegalWrite} from "@erpx/server-legal/application/service";
export type DocumentBytesObject={size:number;arrayBuffer():Promise<ArrayBuffer>};
export type DocumentStorage={get(key:string):Promise<DocumentBytesObject|null>;head(key:string):Promise<{etag:string}|null>;put(key:string,bytes:Uint8Array,options:{onlyIf:{etagDoesNotMatch:string};httpMetadata:{contentType:string};customMetadata:Record<string,string>}):Promise<unknown>};
export type DocumentIntake={readOnly():boolean;createSet(input:{targetType:"legal_item";targetId:string;componentKind:"record";componentRef:string;purposeKey:"legal_document";expectedTargetVersion:number},key:string,options:{compatibilityAdapter:true}):Promise<{id:string}>;reserve(setId:string,input:{filename:string;mediaType:string;bytes:number},key:string,options:{compatibilityAdapter:true}):Promise<{uploadIntentId:string}>;complete(intentId:string,bytes:ArrayBuffer,options:{compatibilityAdapter:true}):Promise<{sourceId:string;attachmentLinkId:string}>;scan(sourceId:string):Promise<string>;binding(input:{attachmentSetIds:string[];bindingContract:{targetType:"legal_item";bindingType:"legal_document";contexts:{purposeKey:"legal_document";componentKind:"record";componentRef:string}[]};targetId:string;expectedTargetVersion:number;commandId:string;bindingId:string},options:{compatibilityAdapter:true}):Promise<{guards:LegalWrite[];steps:LegalWrite[]}>;content(linkId:string,variant:"original",options:{compatibilityAdapter:true}):Promise<{object:DocumentBytesObject}>};
export type DocumentInspectionSession={write(bytes:Uint8Array):Promise<void>;verify():Promise<Record<string,unknown>>;extract(document:DocumentVersion):Promise<{text:string;pages:{page:number;text:string}[];pageMapping:boolean}>;close():Promise<void>};
export type DocumentProvider={docx(body:string):Promise<Uint8Array>;pdf(body:string):Promise<Uint8Array>;prepare():Promise<DocumentInspectionSession>};
export type LegalDocumentRepository={
approvedVersion(facts:{id:string;}):Promise<{ cycle: number; document_id: string; document_hash: string; } | null>;
retainExecutedCopy(facts:{executedCopyId:string;id:string;documentId:string;cycle:number;document_id:string;document_hash:string;checksum:string;metadataJson:string;actorId:string;now:number;}):LegalWrite;
lockExecutedDocument(facts:{documentId:string;}):LegalWrite;
cleanSource(facts:{sourceId:string;companyId:string;}):Promise<{ object_key: string; checksum_sha256: string; media_type: string; bytes: number; } | null>;
nextVersion(facts:{id:string;}):Promise<{ n: number; } | null>;
insertCanonicalDocument(facts:{documentId:string;id:string;next:number;name:string;media_type:string;checksum_sha256:string;fact6:number;object_key:string;actorId:string;now:number;attachmentLinkId:string;sourceId:string;}):LegalWrite;
advanceCanonicalItem(facts:{now:number;id:string;}):LegalWrite;
nextStoredVersion(facts:{id:string;}):Promise<{ n: number; } | null>;
insertStoredDocument(facts:{identifier:string;id:string;next:number;name:string;type:"application/pdf" | "image/png" | "image/jpeg" | "application/vnd.openxmlformats-officedocument.wordprocessingml.document";checksum:string;length:number;objectKey:string;actorId:string;now:number;}):LegalWrite;
advanceStoredItem(facts:{now:number;id:string;}):LegalWrite;
document(facts:{id:string;}):Promise<DocumentVersion | null>;
disposed(facts:{id:string;}):Promise<Record<string, unknown> | null>;
destroying(facts:{item_id:string;}):Promise<Record<string, unknown> | null>;
recordExport(facts:{identifier:string;item_id:string;id:string;actorId:string;fact4:string;now:number;}):Promise<void>;
preservationLock():LegalWrite;
insertInspection(facts:{inspectionId:string;id:string;kind:"ocr" | "verify";resultJson:string;actorId:string;now:number;item_id:string;}):LegalWrite;
recordInspectionRead(facts:{identifier:string;item_id:string;id:string;actorId:string;fact4:string;now:number;}):Promise<void>;
submission(facts:{id:string;}):Promise<{ id: string; item_id: string; object_key: string; checksum: string; name: string; media_type: string; bytes: number; attachment_link_id: string | null; attachment_source_id: string | null; } | null>;
recordSubmissionRead(facts:{identifier:string;item_id:string;null:null;actorId:string;fact4:"external-submission.download";now:number;}):Promise<void>;
requireExecutedApproval(key:string,values:(string | number)[]):LegalWrite;
requireExecutedAuthority(key:string,documentId:string,values:string[]):LegalWrite;
persistInspection(writes:LegalWrite[]):Promise<number>;
guardChanges(id:string,version:number,key:string,edit?:boolean,allowFrozen?:boolean,preparation?:boolean):LegalWrite[];
event(id:string,action:string,note:string,version:number,identifier:string,now:number):LegalWrite;
replay<Result extends object=Record<string,unknown>>(key:string,operation:string,id:string,input:unknown):Promise<Result|null>;
commit<Result extends object>(key:string,operation:string,input:unknown,id:string,version:number,result:Result,writes:LegalWrite[]):Promise<Result>;
};
export function createLegalDocumentApplication(ports:{actor:WorkContext;repository:LegalDocumentRepository;storage:DocumentStorage;fileIntake:DocumentIntake;provider:DocumentProvider;clock:Clock;identifiers:IdentifierSource;hash(bytes:string|Uint8Array):string;utf8(value:string):Uint8Array;ascii(bytes:Uint8Array):string;hex(bytes:Uint8Array):string;access(id:string,permission?:string):Promise<LegalItem>;error(status:number,code:string,message:string):Error}){const a=ports.actor;function fail(message:string,code="LEGAL_INVALID",status=422):never{throw ports.error(status,code,message);}
async function uploadDocument(
  id: string,
  version: number,
  name: string,
  type: string,
  bytes: Uint8Array,
  key: string,
  executed?: ExecutedCopyInput,
): Promise<{id:string;version:number;documentId?:string;executedCopyId?:string;state?:string}> {
  const r = await ports.access(id,"legal.edit");
  if(executed)executed=executedCopySchema.parse(executed);
  z.string().min(1).max(200).parse(name);
  if (!bytes.length || bytes.length > 10 * 1024 * 1024)
    fail("Upload between 1 byte and 10 MB.");
  const allowed =
    (type === "application/pdf" &&
      ports.ascii(bytes.subarray(0,5)) === "%PDF-") ||
    (type === "image/png" &&
      ports.hex(bytes.subarray(0,8)) ===
        "89504e470d0a1a0a") ||
    (type === "image/jpeg" &&
      bytes[0] === 255 &&
      bytes[1] === 216 &&
      bytes[2] === 255) ||
    (type ===
      "application/vnd.openxmlformats-officedocument.wordprocessingml.document" &&
      bytes[0] === 80 &&
      bytes[1] === 75);
  if (!allowed)
    fail("Upload a PDF, PNG, JPEG or DOCX with a matching file signature.");
  const checksum = ports.hash(bytes),
    input = { id, version, name, type, checksum, ...(executed?{executed}:{}) },
    operation=executed?"legal.executed.import":"legal.document.upload",
    prior = await ports.repository.replay<{ id: string; version: number; documentId?: string; executedCopyId?: string; state?: string }>(key,operation,id,input);
  if (prior) return prior;
  if (executed ? r.kind!=="contract"||!["approved","executed","active"].includes(r.state) : r.state !== "draft") fail(executed?"Import a signed copy against an approved contract.":"Return to draft before adding a version.");
  if (r.version !== version)
    fail("The record changed. Refresh first.", "LEGAL_VERSION_CONFLICT", 409);
  const approved=executed?await ports.repository.approvedVersion({id:id}):null;
  if(executed&&(!approved||approved.document_id!==executed.approvedDocumentId))fail("Choose the current approved document.","LEGAL_APPROVAL_STALE",409);
  if(executed){
    if(type!=="application/pdf"&&!type.startsWith("image/"))fail("Import an executed copy as a PDF or scanned image.");
    if(executed.signedDate>new Intl.DateTimeFormat("en-CA",{timeZone:r.timezone,year:"numeric",month:"2-digit",day:"2-digit"}).format(ports.clock.now()))fail("The signing date cannot be in the future.");
    for(const documentId of new Set(executed.signers.flatMap(s=>s.authorityDocumentIds))){
      const d=await documentAccess(documentId);
      if(d.item_id!==id)fail("Authority evidence must be attached to this contract.");
    }
  }
  const executedCopyId=executed?ports.identifiers.next():null;
  const executionSteps=(documentId:string)=>executed&&approved&&executedCopyId?[
    ports.repository.requireExecutedApproval(key,[id,approved.cycle,approved.document_id,approved.document_hash,id,approved.cycle]),
    ...Array.from(new Set(executed.signers.flatMap(s=>s.authorityDocumentIds))).map(d=>ports.repository.requireExecutedAuthority(key,d,[d,id,d])),
    ports.repository.retainExecutedCopy({executedCopyId:executedCopyId,id:id,documentId:documentId,cycle:approved.cycle,document_id:approved.document_id,document_hash:approved.document_hash,checksum:checksum,metadataJson:JSON.stringify(executed),actorId:a.userId,now:ports.clock.now()}),
    ports.repository.lockExecutedDocument({documentId:documentId}),
  ]:[];
  if(ports.fileIntake.readOnly())fail("Attachment intake is disabled during rollback.","ATTACHMENTS_READ_ONLY",503);
  if (!ports.fileIntake.readOnly()) {
    const set = await ports.fileIntake.createSet({
        targetType: "legal_item",
        targetId: id,
        componentKind: "record",
        componentRef: key,
        purposeKey: "legal_document",
        expectedTargetVersion: version,
      },`${key}:set`,{ compatibilityAdapter: true });
    const reserved = await ports.fileIntake.reserve(set.id,{ filename: name, mediaType: type, bytes: bytes.byteLength },`${key}:file`,{ compatibilityAdapter: true });
    const uploaded = await ports.fileIntake.complete(reserved.uploadIntentId,Uint8Array.from(bytes).buffer,{ compatibilityAdapter: true });
    const sourceState = await ports.fileIntake.scan(uploaded.sourceId);
    if (sourceState !== "available")
      fail(
        sourceState === "malware_rejected"
          ? "The upload was rejected by malware scanning."
          : "The upload could not pass malware scanning.",
        sourceState === "malware_rejected"
          ? "LEGAL_UPLOAD_REJECTED"
          : "LEGAL_SCAN_UNAVAILABLE",
        sourceState === "malware_rejected" ? 422 : 503,
      );
    const source = await ports.repository.cleanSource({sourceId:uploaded.sourceId,companyId:a.companyId});
    if (!source || source.checksum_sha256 !== checksum)
      fail("Stored file integrity check failed.", "LEGAL_INTEGRITY", 409);
    const documentId = ports.identifiers.next();
    const binding = await ports.fileIntake.binding({
      attachmentSetIds: [set.id],
      bindingContract: {
        targetType: "legal_item",
        bindingType: "legal_document",
        contexts: [{ purposeKey: "legal_document", componentKind: "record", componentRef: key }],
      },
      targetId: id,
      expectedTargetVersion: version,
      commandId: key,
      bindingId: documentId,
    },{compatibilityAdapter:true});
    const next = (await ports.repository.nextVersion({id:id}))!.n;
    return ports.repository.commit(
      key,
      operation,
      input,
      id,
      version + 1,
      { id, version: version + 1, documentId, ...(executedCopyId?{executedCopyId,state:r.state}:{}) },
      [
        ...ports.repository.guardChanges(id,version,key,true,false,["rule","template"].includes(r.kind)),
        ...binding.guards,
        ports.repository.insertCanonicalDocument({documentId:documentId,id:id,next:next,name:name,media_type:source.media_type,checksum_sha256:source.checksum_sha256,fact6:Number(source.bytes),object_key:source.object_key,actorId:a.userId,now:ports.clock.now(),attachmentLinkId:uploaded.attachmentLinkId,sourceId:uploaded.sourceId}),
        ports.repository.advanceCanonicalItem({now:ports.clock.now(),id:id}),
        ...binding.steps,
        ...executionSteps(documentId),
        ports.repository.event(id,executed?"executed.import":"document.upload",`${name} · ${checksum}`,version + 1,ports.identifiers.next(),ports.clock.now()),
      ],
    );
  }
  const objectKey = `legal/${a.companyId}/${id}/${key}/${checksum}`;
  await ports.storage.put(objectKey, bytes, {
    onlyIf: { etagDoesNotMatch: "*" },
    httpMetadata: { contentType: type },
    customMetadata: { legalItem: id, companyId: a.companyId, checksum },
  });
  const stored = await ports.storage.head(objectKey);
  if (stored?.etag !== checksum) fail("Stored file integrity check failed.");
  const next = (await ports.repository.nextStoredVersion({id:id}))!.n;
  return ports.repository.commit(
    key,
    "legal.document.upload",
    input,
    id,
    version + 1,
    { id, version: version + 1 },
    [
      ...ports.repository.guardChanges(id,version,key,true,false,["rule","template"].includes(r.kind)),
      ports.repository.insertStoredDocument({identifier:ports.identifiers.next(),id:id,next:next,name:name,type:type,checksum:checksum,length:bytes.length,objectKey:objectKey,actorId:a.userId,now:ports.clock.now()}),
      ports.repository.advanceStoredItem({now:ports.clock.now(),id:id}),
      ports.repository.event(id,"document.upload",`${name} · ${checksum}`,version + 1,ports.identifiers.next(),ports.clock.now()),
    ],
  );
}
async function importExecutedCopy(id:string,version:number,metadata:unknown,name:string,type:string,bytes:Uint8Array,key:string){return uploadDocument(id,version,name,type,bytes,key,executedCopySchema.parse(metadata));}
async function documentAccess(
  id: string,
  permission = "legal.read",
) {
  const d = await ports.repository.document({id:id});
  if (!d) fail("Document unavailable.", "LEGAL_NOT_FOUND", 404);
  await ports.access(d.item_id,permission);
  if (
    await ports.repository.disposed({id:id})
  )
    fail(
      "Document content was destroyed under an approved retention policy. Metadata and destruction evidence remain.",
      "LEGAL_DOCUMENT_DISPOSED",
      410,
    );
  if(await ports.repository.destroying({item_id:d.item_id}))
    fail("Document retention is in progress. Review or resume the preserved manifest before reading content.","LEGAL_RETENTION_FROZEN",409);
  return d;
}
function documentBytes(d:DocumentVersion){return readLegalDocumentBytes(ports,d);}
async function authorizedDocumentBytes(
  d: DocumentVersion,
) {
  if (d.attachment_link_id) {
    const canonical = await ports.fileIntake.content(d.attachment_link_id,"original",{compatibilityAdapter:true});
    const bytes = new Uint8Array(await canonical.object.arrayBuffer());
    if (ports.hash(bytes) !== d.checksum)
      fail("Document integrity check failed.", "LEGAL_INTEGRITY", 409);
    return bytes;
  }
  if (d.object_key)
    fail(
      "This document has not been migrated to the current attachment lifecycle.",
      "LEGAL_ATTACHMENT_LINK_REQUIRED",
      404,
    );
  return documentBytes( d);
}
async function documentExport(
  id: string,
  format: string | null,
) {
  const d = await documentAccess( id, "legal.export");
  let bytes: Uint8Array = await authorizedDocumentBytes(d);
  let type = d.media_type,
    name = d.name;
  if(!d.object_key&&format==="docx"){bytes=await ports.provider.docx(d.body);type="application/vnd.openxmlformats-officedocument.wordprocessingml.document";name+=".docx";}
  else if(!d.object_key&&format==="pdf"){bytes=await ports.provider.pdf(d.body);type="application/pdf";name+=".pdf";}
  else if(!d.object_key)name+=".txt";
  await ports.repository.recordExport({identifier:ports.identifiers.next(),item_id:d.item_id,id:d.id,actorId:a.userId,fact4:`document.export.${format ?? "original"}`,now:ports.clock.now()});
  return {bytes,type,name};
}
async function inspectDocument(id:string,kind:"ocr"|"verify"){
 const document=await documentAccess(id),bytes=await authorizedDocumentBytes(document),session=await ports.provider.prepare();
 try{
  await session.write(bytes);
  let result:Record<string,unknown>;
  if(kind==="verify"){if(document.media_type!=="application/pdf")fail("Signature inspection requires a PDF.");result={...await session.verify(),checksum:document.checksum};}
  else{const extracted=await session.extract(document),text=extracted.text;let pages=extracted.pages;if(!pages.length)pages=[{page:1,text:text.slice(0,200000)}];let remaining=200000;pages=pages.map(page=>{const text=page.text.slice(0,remaining);remaining-=text.length;return {...page,text};});result={text:text.slice(0,200000),pages,pageMapping:document.object_key?extracted.pageMapping:false,truncated:text.length>200000,checksum:document.checksum,requiresReview:true};}
  await documentAccess(id);const inspectionId=ports.identifiers.next();result={...result,inspectionId};
  const changes=await ports.repository.persistInspection([ports.repository.preservationLock(),ports.repository.insertInspection({inspectionId,id:document.id,kind,resultJson:JSON.stringify(result),actorId:a.userId,now:ports.clock.now(),item_id:document.item_id})]);
  if(changes!==1)fail("Document preservation changed while inspecting. Refresh first.","LEGAL_DOCUMENT_FROZEN",409);
  await ports.repository.recordInspectionRead({identifier:ports.identifiers.next(),item_id:document.item_id,id:document.id,actorId:a.userId,fact4:`document.${kind}`,now:ports.clock.now()});return result;
 }catch(error){if(error instanceof Error&&"code"in error&&["ENOENT","ETIMEDOUT"].includes(String(error.code)))fail("The document tool is unavailable or timed out.","LEGAL_TOOL_UNAVAILABLE",503);throw error;}
 finally{await session.close();}
}
async function externalSubmission(
  id: string,
) {
  const row = await ports.repository.submission({id:id});
  if (!row) fail("Submission unavailable.", "LEGAL_NOT_FOUND", 404);
  await ports.access(row.item_id,"legal.export");
  if (!row.attachment_link_id)
    fail(
      "This submission has not been migrated to the current attachment lifecycle.",
      "LEGAL_ATTACHMENT_LINK_REQUIRED",
      404,
    );
  const canonical =
    row.attachment_link_id
      ? await ports.fileIntake.content(row.attachment_link_id,"original",{compatibilityAdapter:true})
      : null;
  const f = canonical?.object ?? (await ports.storage.get(row.object_key));
  if (!f) fail("File unavailable.", "LEGAL_STORAGE_UNAVAILABLE", 503);
  const bytes = new Uint8Array(await f.arrayBuffer());
  if (ports.hash(bytes) !== row.checksum) fail("File integrity check failed.");
  await ports.repository.recordSubmissionRead({identifier:ports.identifiers.next(),item_id:row.item_id,null:null,actorId:a.userId,fact4:"external-submission.download",now:ports.clock.now()});
  return { row, bytes };
}

return {uploadDocument,importExecutedCopy,documentAccess,documentBytes,documentExport,inspectDocument,externalSubmission};}

export async function readLegalDocumentBytes(ports:{storage:Pick<DocumentStorage,"get">;utf8(value:string):Uint8Array;hash(bytes:Uint8Array):string;error(status:number,code:string,message:string):Error},d:DocumentVersion) {
  if (!d.object_key) {
    const bytes = ports.utf8(d.body);
    if (ports.hash(bytes) !== d.checksum)
      throw ports.error(409,"LEGAL_INTEGRITY","Document integrity check failed.");
    return bytes;
  }
  const file = await ports.storage.get(d.object_key);
  if (!file)
    throw ports.error(503,"LEGAL_STORAGE_UNAVAILABLE","Document storage is unavailable.");
  const bytes = new Uint8Array(await file.arrayBuffer());
  if (ports.hash(bytes) !== d.checksum)
    throw ports.error(409,"LEGAL_INTEGRITY","Document integrity check failed.");
  return bytes;
}
