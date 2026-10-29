import {z} from "zod";
import type {Clock,IdentifierSource} from "@erpx/server-shared/application/ports";
import type {DocumentVersion} from "@erpx/server-legal/domain/model";
import type {PortalActor} from "@erpx/server-legal/domain/portal";
import {refuseLegalSigning} from "@erpx/server-legal/domain/signing";
import type {PortalAttachment,PortalScope} from "@erpx/server-legal/domain/portal-attachments";
import {legalPortalUploadFingerprintInput,type LegalPortalUploadNotApplied} from '@erpx/contracts/legal-portal-upload';
import type {PortalAttachmentStageScope,PortalAttachmentRecoverySource} from '@erpx/server-legal/application/portal-attachments';
export type LegalPortalWrite={owner:"legal"};
export type LegalPortalRows<Row>={results:Row[]};
export type LegalPortalShare={id:string;item_id:string;document_id:string;company_id:string;branch_id:string|null;department_id:string|null;item_version:number;kind:string;version:number};
export type LegalPortalShareFacts={id:string;email:string;actorId:string;now:number};
export type LegalPortalUnit={
shares(facts:{email:string;actorId:string;now:number;}):Promise<LegalPortalRows<Record<string, unknown>>>;
share(facts:{id:string;email:string;actorId:string;now:number;}):Promise<{ id: string; item_id: string; document_id: string; company_id: string; branch_id: string | null; department_id: string | null; item_version: number; kind: string; version: number; } | null>;
uploadRecoveryShare(facts:LegalPortalShareFacts):Promise<LegalPortalShare|null>;
uploadRecoverySource(facts:PortalAttachmentStageScope):Promise<PortalAttachmentRecoverySource|null>;
retentionFrozen(facts:{item_id:string;}):Promise<Record<string, unknown> | null>;
documentSummary(facts:{document_id:string;}):Promise<Record<string, unknown> | null>;
comments(facts:{item_id:string;document_id:string;}):Promise<LegalPortalRows<Record<string, unknown>>>;
envelopes(facts:{item_id:string;document_id:string;actorId:string;}):Promise<LegalPortalRows<Record<string, unknown>>>;
viewRead(facts:{identifier:string;item_id:string;document_id:string;actorId:string;fact4:"portal.view";now:number;}):Promise<LegalPortalRows<Record<string, unknown>>>;
billing(facts:{item_id:string;actorId:string;}):Promise<LegalPortalRows<{id:string;title:string;state:string;data:unknown;version:number}>>;
document(facts:{document_id:string;}):Promise<DocumentVersion | null>;
downloadRead(facts:{identifier:string;item_id:string;id:string;actorId:string;fact4:"portal.download";now:number;}):Promise<LegalPortalRows<Record<string, unknown>>>;
receipt(facts:{key:string;}):Promise<{ user_id: string; request_hash: string; result_json: string; } | null>;
enabled(facts:{company_id:string;}):Promise<Record<string, unknown> | null>;
acceptShare(facts:{actorId:string;id:string;}):Promise<LegalPortalRows<Record<string, unknown>>>;
externalComment(facts:{identifier:string;item_id:string;document_id:string;body:string;actorId:string;now:number;}):Promise<LegalPortalRows<Record<string, unknown>>>;
counselEntry(facts:{key:string;item_id:string;title:string;actorId:string;fact4:string;actorId2:string;actorId3:string;now:number;}):Promise<LegalPortalRows<Record<string, unknown>>>;
counselHistory(facts:{identifier:string;item_id:string;key:string;fact3:string;actorId:string;now:number;}):Promise<LegalPortalRows<Record<string, unknown>>>;
advanceItem(facts:{now:number;item_id:string;}):Promise<LegalPortalRows<Record<string, unknown>>>;
actionableEnvelope(facts:{envelopeId:string | undefined;item_id:string;document_id:string;actorId:string;now:number;}):Promise<{ id: string; ordinal: number; viewed_at: number | null; consent: string; document_hash: string; } | null>;
viewEnvelope(facts:{now:number;id:string;actorId:string;}):Promise<LegalPortalRows<Record<string, unknown>>>;
interactionEvent(facts:{identifier:string;fact1:string;actorId:string;document_id:string;now:number;item_id:string;}):Promise<LegalPortalRows<Record<string, unknown>>>;
interactionReceipt(facts:{key:string;actorId:string;digest:string;fact3:string;now:number;}):Promise<LegalPortalRows<Record<string, unknown>>>;
uploadReceipt(facts:{key:string;actorId:string;}):Promise<{ request_hash: string; result_json: string; } | null>;
uploadEnabled(facts:{company_id:string;}):Promise<Record<string, unknown> | null>;
uploadReplay(facts:{key:string;actorId:string;}):Promise<{ request_hash: string; result_json: string; } | null>;
externalUpload(facts:{key:string;shareId:string;objectKey:string;checksum:string;name:string;mediaType:string;bytes:number;actorId:string;now:number;linkId:string;sourceId:string;}):Promise<LegalPortalRows<Record<string, unknown>>>;
bindLink(facts:{now:number;key:string;linkId:string;sourceId:string;}):LegalPortalWrite;
bindSet(facts:{now:number;key:string;now2:number;setId:string;expectedTargetVersion:number;}):LegalPortalWrite;
advanceUploadItem(facts:{now:number;item_id:string;company_id:string;expectedTargetVersion:number;}):LegalPortalWrite;
bindEvent(facts:{fact0:string;company_id:string;sourceId:string;linkId:string;setId:string;actorId:string;fact6:"set.bound";fact7:string;now:number;}):LegalPortalWrite;
uploadEvent(facts:{identifier:string;actorId:string;fact2:string;now:number;item_id:string;}):Promise<LegalPortalRows<Record<string, unknown>>>;
uploadCommandReceipt(facts:{key:string;actorId:string;digest:string;fact3:string;now:number;}):Promise<LegalPortalRows<Record<string, unknown>>>;
item(facts:{itemId:string;}):Promise<{ company_id: string; branch_id: string | null; department_id: string | null; version: number; } | null>;
audit(facts:{identifier:string;key:string;actorId:string;fact3:"external-share";action:string;fact5:"legal";itemId:string;version:number;fact8:"Shared document interaction";now:number;authority:string;}):Promise<LegalPortalRows<Record<string, unknown>>>;
outbox(facts:{identifier:string;key:string;fact2:string;fact3:"legal";itemId:string;version:number;fact6:string;now:number;}):Promise<LegalPortalRows<Record<string, unknown>>>;
lock(companyId:string):Promise<unknown>;
batch(writes:LegalPortalWrite[]):Promise<unknown>;
};
export type LegalPortalRepository=LegalPortalUnit&{transaction<Result>(work:(unit:LegalPortalUnit)=>Promise<Result>):Promise<Result>};
export function createLegalPortalApplication(ports:{requestUploadReplay?:boolean;actor:PortalActor;repository:LegalPortalRepository;clock:Clock;identifiers:IdentifierSource;hash(bytes:string|Uint8Array):string;decode(bytes:Uint8Array):string;error(status:number,code:string,message:string):Error;attachments:{read(input:{companyId:string;itemId:string;linkId:string;actorId:string;purposeKey:"legal_document"}):Promise<{bytes:Uint8Array;mediaType:string;filename:string}>;stage(actorId:string,scope:PortalScope,name:string,mediaType:string,bytes:Uint8Array,key:string):Promise<PortalAttachment>}}){const a=ports.actor;function fail(message:string,code="LEGAL_INVALID",status=422):never{throw ports.error(status,code,message);}
async function portalList() {
  return (
    await ports.repository.shares({email:a.email,actorId:a.userId,now:ports.clock.now()})
  ).results;
}
async function share(unit:LegalPortalUnit,id: string,recovery=false) {
  const facts={id,email:a.email,actorId:a.userId,now:ports.clock.now()},s = await (recovery?unit.uploadRecoveryShare(facts):unit.share(facts));
  if (!s)
    fail(
      "This share expired or is unavailable.",
      "LEGAL_SHARE_UNAVAILABLE",
      404,
    );
  if (
    await unit.retentionFrozen({item_id:s.item_id})
  )
    fail("Document retention is in progress.", "LEGAL_RETENTION_FROZEN", 409);
  return s;
}
async function portalDetail(
  id: string,
) {
  const s = await share(ports.repository,id),
    doc = await ports.repository.documentSummary({document_id:s.document_id});
  const comments = (
    await ports.repository.comments({item_id:s.item_id,document_id:s.document_id})
  ).results;
  const envelopes = (
    await ports.repository.envelopes({item_id:s.item_id,document_id:s.document_id,actorId:a.userId})
  ).results;
  await ports.repository.viewRead({identifier:ports.identifiers.next(),item_id:s.item_id,document_id:s.document_id,actorId:a.userId,fact4:"portal.view",now:ports.clock.now()});
  const billing =
    s.kind === "counsel"
      ? (
          await ports.repository.billing({item_id:s.item_id,actorId:a.userId})
        ).results.map((r) => ({
          ...r,
          data: typeof r.data === "string" ? JSON.parse(r.data) : r.data,
        }))
      : [];
  return { shareId: id, doc, comments, envelopes, kind: s.kind, billing };
}
async function portalDownload(
  id: string,
) {
  const s = await share(ports.repository,id),
    d = await ports.repository.document({document_id:s.document_id});
  if (!d) fail("Document unavailable.", "LEGAL_NOT_FOUND", 404);
  if (d.object_key && !d.attachment_link_id)
    fail(
      "This document has not been migrated to the current attachment lifecycle.",
      "LEGAL_ATTACHMENT_LINK_REQUIRED",
      404,
    );
  const canonical = d.attachment_link_id
      ? await ports.attachments.read({
          companyId: s.company_id,
          itemId: s.item_id,
          linkId: d.attachment_link_id,
          actorId: a.userId,
          purposeKey: "legal_document",
        })
      : null;
  if (!canonical)
    fail(
      "This document has not been migrated to the current attachment lifecycle.",
      "LEGAL_ATTACHMENT_LINK_REQUIRED",
      404,
    );
  const bytes = canonical.bytes;
  await share(ports.repository,id);
  await ports.repository.downloadRead({identifier:ports.identifiers.next(),item_id:s.item_id,id:d.id,actorId:a.userId,fact4:"portal.download",now:ports.clock.now()});
  return {bytes:new Uint8Array(bytes),mediaType:canonical.mediaType,filename:d.name};
}
async function portalCommand(
  raw: unknown,
  key: string,
) {
  const x = z
    .object({
      shareId: z.string(),
      action: z.enum([
        "accept",
        "comment",
        "view",
        "sign",
        "decline",
        "billing.submit",
      ]),
      envelopeId: z.string().optional(),
      body: z.string().min(1).max(20000).optional(),
      consent: z.string().optional(),
    })
    .parse(raw);
  z.string().uuid().parse(key);
  if(["sign","decline"].includes(x.action))refuseLegalSigning(ports.error);
  const digest = ports.hash(JSON.stringify({ user: a.userId, input: raw }));
  return ports.repository.transaction(async (tx) => {
    const s = await share(tx,x.shareId);
    await tx.lock(s.company_id);
    await share(tx,x.shareId);
    const prior = await tx.receipt({key:key});
    if (prior) {
      if (prior.user_id !== a.userId || prior.request_hash !== digest)
        fail(
          "This command key belongs to another request.",
          "IDEMPOTENCY_CONFLICT",
          409,
        );
      return JSON.parse(prior.result_json);
    }
    if (
      !(await tx.enabled({company_id:s.company_id}))
    )
      fail("Legal is currently read-only.");
    if (x.action === "accept")
      await tx.acceptShare({actorId:a.userId,id:s.id});
    else if (x.action === "comment") {
      if (!x.body) fail("Enter a comment.");
      await tx.externalComment({identifier:ports.identifiers.next(),item_id:s.item_id,document_id:s.document_id,body:x.body,actorId:a.userId,now:ports.clock.now()});
    } else if (x.action === "billing.submit") {
      if (s.kind !== "counsel")
        fail("Billing submissions require a counsel engagement share.");
      const p = z
        .object({
          entryType: z.enum(["time", "fee", "invoice"]),
          description: z.string().trim().min(3).max(12000),
          minutes: z.coerce.number().int().min(0).max(100000),
          amount: z.coerce.number().int().min(0).max(Number.MAX_SAFE_INTEGER),
          evidence: z.string().trim().min(3).max(12000),
        })
        .parse(raw);
      if (p.entryType === "time" && p.amount !== 0)
        fail("Submit charges separately as a fee or invoice.");
      const entry = {
        id: key,
        item_id: s.item_id,
        category: "counsel_entry",
        title: p.description.slice(0, 200),
        owner_id: a.userId,
        due_date: null,
        state: "submitted",
        version: 1,
        data: {
          entryType: p.entryType,
          description: p.description,
          minutes: String(p.minutes),
          amount: String(p.amount),
          evidence: p.evidence,
        },
        created_by: a.userId,
        updated_by: a.userId,
        updated_at: ports.clock.now(),
      };
      await tx.counselEntry({key:key,item_id:s.item_id,title:entry.title,actorId:a.userId,fact4:JSON.stringify(entry.data),actorId2:a.userId,actorId3:a.userId,now:ports.clock.now()});
      await tx.counselHistory({identifier:ports.identifiers.next(),item_id:s.item_id,key:key,fact3:JSON.stringify(entry),actorId:a.userId,now:ports.clock.now()});
      await tx.advanceItem({now:ports.clock.now(),item_id:s.item_id});
    } else {
      const e = await tx.actionableEnvelope({envelopeId:x.envelopeId,item_id:s.item_id,document_id:s.document_id,actorId:a.userId,now:ports.clock.now()});
      if (!e)
        fail("Envelope is no longer actionable.", "LEGAL_ENVELOPE_STATE", 409);
      if (x.action === "view")
        await tx.viewEnvelope({now:ports.clock.now(),id:e.id,actorId:a.userId});
    }
    await tx.interactionEvent({identifier:ports.identifiers.next(),fact1:`portal.${x.action}`,actorId:a.userId,document_id:s.document_id,now:ports.clock.now(),item_id:s.item_id});
    await tx.interactionReceipt({key:key,actorId:a.userId,digest:digest,fact3:JSON.stringify({ id: s.id }),now:ports.clock.now()});
    await portalOutbox(tx, s.item_id, key, `portal.${x.action}`);
    return { id: s.id };
  });
}

function uploadInput(
  shareId: string,
  name: string,
  mediaType: string,
  bytes: Uint8Array,
  key: string,
) {
  z.string().uuid().parse(key);
  z.string().min(1).max(200).parse(name);
  if (!bytes.length || bytes.length > 10 * 1024 * 1024)
    fail("Upload between 1 byte and 10 MB.");
  if (
    !(
      mediaType === "application/pdf" &&
      ports.decode(bytes.subarray(0,5)) === "%PDF-"
    ) &&
    !(
      mediaType ===
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" &&
      bytes[0] === 80 &&
      bytes[1] === 75
    )
  )
    fail("Upload a PDF or DOCX.");
  const checksum = ports.hash(bytes),digest = ports.hash(
      JSON.stringify({ userId: a.userId, shareId, name, mediaType, checksum }),
    );
  return {checksum,digest};
}

/** Read only. The adapter supplies a fresh source-leased owner read and repeats
 * this private observation after deferred effects before returning its result. */
async function portalUploadRecovery(shareId:string,name:string,mediaType:string,bytes:Uint8Array,key:string):Promise<{result:{id:string}|{uploadOutcome:LegalPortalUploadNotApplied}|null;witness:string}>{
 const {checksum,digest}=uploadInput(shareId,name,mediaType,bytes,key),current=await share(ports.repository,shareId,true),receipt=await ports.repository.receipt({key});
 if(receipt){
  if(receipt.user_id!==a.userId||receipt.request_hash!==digest)fail('Upload key conflict.','IDEMPOTENCY_CONFLICT',409);
  const saved:unknown=JSON.parse(receipt.result_json);
  if(!saved||typeof saved!=='object'||!('id' in saved)||saved.id!==key)fail('The original upload receipt could not be confirmed.','IDEMPOTENCY_CONFLICT',409);
  return {result:{id:key},witness:JSON.stringify({current,receipt})};
 }
 const setId=`attachment-set-legal-portal-${key}`,linkId=`attachment-link-legal-portal-${key}`,uploadId=`attachment-upload-legal-portal-${key}`,sourceId=`attachment-${uploadId}`;
 const stage=await ports.repository.uploadRecoverySource({sourceId,companyId:current.company_id,itemId:current.item_id,shareId,actorId:a.userId,setId,linkId,uploadId,commandId:key,objectKey:`legal/${current.company_id}/${current.item_id}/quarantine/${uploadId}/original`,name}),witness=JSON.stringify({current,receipt,stage});
 if(!stage||stage.scope_matches!==1||stage.recovery_draft!==1||stage.checksum_sha256!==checksum||stage.media_type!==mediaType||Number(stage.bytes)!==bytes.byteLength||typeof stage.expected_target_version!=='number'||!Number.isSafeInteger(stage.expected_target_version)||stage.expected_target_version>=Number(current.item_version))return {result:null,witness};
 // Original owner versions only advance. This exact still-unbound draft can
 // no longer bind at its captured version, and its actor/body has no receipt.
 return {result:{uploadOutcome:{id:key,outcome:'not-applied',requestFingerprint:ports.hash(legalPortalUploadFingerprintInput({shareId,name,mediaType,bytes:bytes.byteLength,checksum}))}},witness};
}

async function portalUpload(shareId:string,name:string,mediaType:string,bytes:Uint8Array,key:string){
  const {checksum,digest}=uploadInput(shareId,name,mediaType,bytes,key),s=await share(ports.repository,shareId);
  const prior = await ports.repository.uploadReceipt({key:key,actorId:a.userId});
  if (prior) {
    if (prior.request_hash !== digest) fail("Upload key conflict.");
    // Normal request recovery repeats its original share and exact receipt in
    // the complete owner transaction. Legacy direct callers retain the same
    // historical fast path; no storage or new business work is repeated.
    if (ports.requestUploadReplay) return ports.repository.transaction(async tx => {
      await tx.lock(s.company_id);
      const current = await share(tx, shareId), receipt = await tx.uploadReplay({key,actorId:a.userId});
      if (current.company_id !== s.company_id || current.item_id !== s.item_id || current.document_id !== s.document_id)
        fail("The original shared upload source changed.", "LEGAL_SHARE_CHANGED", 409);
      if (!receipt || receipt.request_hash !== digest) fail("Upload key conflict.");
      return JSON.parse(receipt.result_json);
    });
    return JSON.parse(prior.result_json);
  }
  const canonical = await ports.attachments.stage(
        a.userId,
        {
          companyId: s.company_id,
          branchId: s.branch_id,
          departmentId: s.department_id,
          itemId: s.item_id,
          itemVersion: Number(s.item_version),
          shareId,
        },
        name,
        mediaType,
        bytes,
        key,
      );
  return ports.repository.transaction(async (tx) => {
    await tx.lock(s.company_id);
    const current = await share(tx,shareId);
    if (
      !(await tx.uploadEnabled({company_id:s.company_id}))
    )
      fail("Legal is read-only.");
    const replay = await tx.uploadReplay({key:key,actorId:a.userId});
    if (replay) {
      if (replay.request_hash !== digest) fail("Upload key conflict.");
      return JSON.parse(replay.result_json);
    }
    if (
      canonical &&
      Number(current.item_version) !== canonical.expectedTargetVersion
    )
      fail(
        "The legal record changed while the upload was being scanned. Upload it again.",
        "LEGAL_VERSION_CONFLICT",
        409,
      );
    await tx.externalUpload({key:key,shareId:shareId,objectKey:canonical.objectKey,checksum:canonical.checksum,name:name,mediaType:canonical.mediaType,bytes:canonical.bytes,actorId:a.userId,now:ports.clock.now(),linkId:canonical.linkId,sourceId:canonical.sourceId});
    {
      const now = ports.clock.now();
      await tx.batch([
        tx.bindLink({now:now,key:key,linkId:canonical.linkId,sourceId:canonical.sourceId}),
        tx.bindSet({now:now,key:key,now2:now,setId:canonical.setId,expectedTargetVersion:canonical.expectedTargetVersion}),
        tx.advanceUploadItem({now:now,item_id:s.item_id,company_id:s.company_id,expectedTargetVersion:canonical.expectedTargetVersion}),
        tx.bindEvent({fact0:`attachment-bind-${key}`,company_id:s.company_id,sourceId:canonical.sourceId,linkId:canonical.linkId,setId:canonical.setId,actorId:a.userId,fact6:"set.bound",fact7:JSON.stringify({
              bindingType: "legal_portal_submission",
              bindingId: key,
              portalShareId: shareId,
            }),now:now}),
      ]);
    }
    await tx.uploadEvent({identifier:ports.identifiers.next(),actorId:a.userId,fact2:`${name} · ${checksum}`,now:ports.clock.now(),item_id:s.item_id});
    await tx.uploadCommandReceipt({key:key,actorId:a.userId,digest:digest,fact3:JSON.stringify({ id: key }),now:ports.clock.now()});
    await portalOutbox(tx, s.item_id, key, "portal.upload");
    return { id: key };
  });
}

async function portalOutbox(
  unit:LegalPortalUnit,
  itemId: string,
  key: string,
  action: string,
) {
  const item = await unit.item({itemId:itemId});
  if (!item) fail("Record unavailable.");
  const authority = JSON.stringify({
    kind: "external-share",
    userId: a.userId,
    sessionId: a.sessionId,
  });
  await unit.audit({identifier:ports.identifiers.next(),key:key,actorId:a.userId,fact3:"external-share",action:action,fact5:"legal",itemId:itemId,version:item.version,fact8:"Shared document interaction",now:ports.clock.now(),authority:authority});
  await unit.outbox({identifier:ports.identifiers.next(),key:key,fact2:`legal.${action}`,fact3:"legal",itemId:itemId,version:item.version,fact6:JSON.stringify({
        id: itemId,
        companyId: item.company_id,
        branchId: item.branch_id,
        departmentId: item.department_id,
        version: item.version,
      }),now:ports.clock.now()});
}

return {portalList,portalDetail,portalDownload,portalCommand,portalUpload,portalUploadRecovery};}
