import {z} from "zod";
import type {WorkContext} from "../../identity_access/public/authorization";
import type {Clock,IdentifierSource} from "../../shared/application/ports";
import {dateSchema,type LegalItem,type DocumentVersion} from "../domain/model";
import {chunksSchema,officialLawUrl,type Chunk,type ReviewRow} from "../domain/law-corpus";
import type {LegalWrite} from "./service";
export type LegalLawCorpusRepository={
cleanSourceLink(facts:{attachment_link_id:string | null | undefined;companyId:string;checksum:string;}):Promise<Record<string, unknown> | null>;
inspection(facts:{inspectionId:string;documentId:string;}):Promise<{ result_json: string; } | null>;
insertChunk(facts:{identifier:string;companyId:string;itemId:string;documentId:string;checksum:string;revisionId:string | null;reviewId:string | null;page:number;pageMapping:boolean;section:string;text:string;textChecksum:string;}):LegalWrite;
registerSource(facts:{key:string;companyId:string;id:string;title:string;fact4:string;actorId:string;now:number;}):LegalWrite;
registeredSource(facts:{sourceId:string;id:string;companyId:string;}):Promise<Record<string, unknown> | null>;
pendingCheck(facts:{sourceCheckId:string;sourceId:string;companyId:string;checksum:string;}):Promise<Record<string, unknown> | null>;
importRevision(facts:{key:string;companyId:string;id:string;sourceId:string;id2:string;checksum:string;revisionLabel:string;publishedOn:string;effectiveFrom:string;fact9:string | null;extractionMethod:"ocr" | "manual";fact11:string | null;actorId:string;now:number;}):LegalWrite;
bindSourceCheck(facts:{key:string;sourceCheckId:string;}):LegalWrite;
importDocumentText(facts:{key:string;companyId:string;id:string;id2:string;checksum:string;extractionMethod:"ocr" | "manual";fact6:string | null;actorId:string;now:number;}):LegalWrite;
approvedRevision(facts:{revisionId:string;id:string;companyId:string;}):Promise<Record<string, unknown> | null>;
proposeApplicability(facts:{key:string;companyId:string;id:string;revisionId:string;applies:boolean;basis:string;actorId:string;now:number;}):LegalWrite;
applicability(facts:{applicabilityId:string;id:string;companyId:string;}):Promise<ReviewRow | null>;
approveApplicability(facts:{actorId:string;now:number;id:string;}):LegalWrite;
review(facts:{id:string;id2:string;companyId:string;reviewKind:"law"|"document";}):Promise<ReviewRow | null>;
decideReview(facts:{fact0:"approved" | "retired";actorId:string;now:number;reason:string;id:string;reviewKind:"law"|"document";}):LegalWrite;
reviewSourceCheck(facts:{id:string;}):LegalWrite;
advanceRecord(facts:{now:number;id:string;}):LegalWrite;
requireLegalCorpusSourceClean(key:string,values:string[]):LegalWrite;
requireLawSourceCurrent(key:string,values:string[]):LegalWrite;
requireLawSourceCheckCurrent(key:string,values:string[]):LegalWrite;
requireLawRevisionCurrent(key:string,values:string[]):LegalWrite;
requireLawApplicabilityCurrent(key:string,values:(string | number)[]):LegalWrite;
requireLawReviewCurrent(key:string,reviewKind:"law"|"document",retiring:boolean,values:(string | number)[]):LegalWrite;
sources(ruleId?:string):Promise<Record<string,unknown>[]>;revisions(ruleId?:string):Promise<Record<string,unknown>[]>;applicabilityList(ruleId?:string):Promise<Record<string,unknown>[]>;textReviews(ruleId?:string):Promise<Record<string,unknown>[]>;checks(ruleId?:string):Promise<Record<string,unknown>[]>;impacts(ruleId?:string):Promise<Record<string,unknown>[]>;chunks(ruleId?:string):Promise<({item_id:string;document_id:string}&Record<string,unknown>)[]>;
recordReads(reads:{identifier:string;itemId:string;documentId:string;now:number}[]):Promise<void>;
guardChanges(id:string,version:number,key:string,edit?:boolean,allowFrozen?:boolean,preparation?:boolean):LegalWrite[];
event(id:string,action:string,note:string,version:number,identifier:string,now:number):LegalWrite;
replay<Result extends object=Record<string,unknown>>(key:string,operation:string,id:string,input:unknown):Promise<Result|null>;
commit<Result extends object>(key:string,operation:string,input:unknown,id:string,version:number,result:Result,writes:LegalWrite[]):Promise<Result>;
};
export function createLegalLawCorpusApplication(ports:{actor:WorkContext;repository:LegalLawCorpusRepository;clock:Clock;identifiers:IdentifierSource;hash(bytes:string):string;require(permission:string):void;samePerson(userId:string|null|undefined):boolean;access(id:string,permission?:string):Promise<LegalItem>;document(id:string):Promise<DocumentVersion>;error(status:number,code:string,message:string):Error}){const a=ports.actor;function fail(message:string,code="LEGAL_INVALID",status=422):never{throw ports.error(status,code,message);}
async function readableSourceDocument(id: string, itemId: string) {
  const d = await ports.document(id);
  if (d.item_id !== itemId) fail("Choose an immutable document from this record.");
  if (d.object_key && !(await ports.repository.cleanSourceLink({attachment_link_id:d.attachment_link_id,companyId:a.companyId,checksum:d.checksum})))
    fail("The original must pass quarantine and scanning before its text can be reviewed.", "LEGAL_SOURCE_QUARANTINED", 409);
  return d;
}
function sourceDocumentGuard(key: string, documentId: string, itemId: string, checksum: string) {
  return ports.repository.requireLegalCorpusSourceClean(key,[documentId, itemId, a.companyId, checksum]);
}
async function extractionEvidence(documentId: string, checksum: string, method: string, inspectionId?: string) {
  if (method === "ocr" && !inspectionId) fail("Select the extraction inspection for this immutable original.");
  if (!inspectionId) return null;
  const row = await ports.repository.inspection({inspectionId:inspectionId,documentId:documentId});
  const result = row ? (typeof row.result_json === "string" ? JSON.parse(row.result_json) : row.result_json) : null;
  if (!result || result.checksum !== checksum) fail("The extraction does not match this immutable original.", "LEGAL_SOURCE_CHECKSUM", 409);
  return result as { pageMapping?: boolean };
}
function insertChunks(itemId: string, documentId: string, checksum: string, chunks: Chunk[], revisionId: string | null, reviewId: string | null): LegalWrite[] {
  return chunks.map((chunk) => ports.repository.insertChunk({identifier:ports.identifiers.next(),companyId:a.companyId,itemId:itemId,documentId:documentId,checksum:checksum,revisionId:revisionId,reviewId:reviewId,page:chunk.page,pageMapping:chunk.pageMapping,section:chunk.section,text:chunk.text,textChecksum:ports.hash(chunk.text)}));
}
async function lawCorpusCommand(raw: unknown, key: string) {
  const x = z.object({ action: z.enum(["law.source.register", "law.revision.import", "law.revision.approve", "law.revision.retire", "law.applicability.propose", "law.applicability.approve", "knowledge.document.import", "knowledge.document.approve"]), id: z.string().min(1), expectedVersion: z.number().int().positive(), reason: z.string().trim().min(3).max(4000) }).parse(raw);
  const approving = x.action.endsWith("approve") || x.action.endsWith("retire");
  ports.require(x.action === "law.source.register" ? "legal.manage" : approving ? "legal.approve" : "legal.edit");
  if (x.action.startsWith("law.") && (a.branchId || a.departmentId)) fail("Manage the law corpus through a Company assignment.", "LEGAL_COMPANY_SCOPE", 403);
  const item = await ports.access(x.id,approving ? "legal.read" : "legal.edit");
  if (x.action.startsWith("law.") && item.kind !== "rule") fail("Attach official law evidence to a jurisdiction rule.");
  if (x.action.startsWith("knowledge.") && item.kind === "rule") fail("Official law text requires a law revision and a separate Company applicability review.");
  const prior = await ports.repository.replay<{id:string;evidenceId:string;version:number;state:string}>(key,x.action,item.id,raw);
  if (prior) return prior;
  if (item.version !== x.expectedVersion) fail("The record changed. Refresh first.", "LEGAL_VERSION_CONFLICT", 409);
  const steps = ports.repository.guardChanges(item.id,x.expectedVersion,key,!approving,false,x.action.startsWith("law."));
  let evidenceId = key;
  if (x.action === "law.source.register") {
    const p = z.object({ sourceUrl: z.string().max(2000), title: z.string().trim().min(3).max(300) }).parse(raw);
    steps.push(ports.repository.registerSource({key:key,companyId:a.companyId,id:item.id,title:p.title,fact4:officialLawUrl(p.sourceUrl,ports.error),actorId:a.userId,now:ports.clock.now()}));
  } else if (x.action === "law.revision.import" || x.action === "knowledge.document.import") {
    const p = z.object({ documentId: z.string(), extractionMethod: z.enum(["manual", "ocr"]), inspectionId: z.string().optional(), chunks: chunksSchema }).parse(raw);
    if (p.chunks.reduce((total, chunk) => total + chunk.text.length, 0) > 200000) fail("Reviewed text is limited to 200,000 characters per import.");
    const d = await readableSourceDocument( p.documentId, item.id);
    steps.push(sourceDocumentGuard( key, d.id, item.id, d.checksum));
    const extraction = await extractionEvidence( d.id, d.checksum, p.extractionMethod, p.inspectionId);
    if ((p.extractionMethod === "ocr" && extraction?.pageMapping !== true) || d.media_type === "application/vnd.openxmlformats-officedocument.wordprocessingml.document") p.chunks = p.chunks.map((chunk) => ({ ...chunk, pageMapping: false }));
    if (x.action === "law.revision.import") {
      const r = z.object({ sourceId: z.string(), sourceCheckId: z.string().optional(), revisionLabel: z.string().trim().min(1).max(200), publishedOn: dateSchema, effectiveFrom: dateSchema, effectiveUntil: dateSchema.optional() }).parse(raw);
      if (r.effectiveUntil && r.effectiveUntil <= r.effectiveFrom) fail("Effective date range is reversed.");
      if (!(await ports.repository.registeredSource({sourceId:r.sourceId,id:item.id,companyId:a.companyId}))) fail("Choose this rule's registered official source.");
      if (r.sourceCheckId && !(await ports.repository.pendingCheck({sourceCheckId:r.sourceCheckId,sourceId:r.sourceId,companyId:a.companyId,checksum:d.checksum}))) fail("The uploaded original must match the pending official-source download checksum.", "LEGAL_SOURCE_CHECKSUM", 409);
      steps.push(ports.repository.requireLawSourceCurrent(key,[r.sourceId, a.companyId, item.id]), ports.repository.importRevision({key:key,companyId:a.companyId,id:item.id,sourceId:r.sourceId,id2:d.id,checksum:d.checksum,revisionLabel:r.revisionLabel,publishedOn:r.publishedOn,effectiveFrom:r.effectiveFrom,fact9:r.effectiveUntil ?? null,extractionMethod:p.extractionMethod,fact11:p.inspectionId ?? null,actorId:a.userId,now:ports.clock.now()}), ...insertChunks( item.id, d.id, d.checksum, p.chunks, key, null));
      if (r.sourceCheckId) steps.push(ports.repository.requireLawSourceCheckCurrent(key,[r.sourceCheckId, d.checksum]), ports.repository.bindSourceCheck({key:key,sourceCheckId:r.sourceCheckId}));
    } else steps.push(ports.repository.importDocumentText({key:key,companyId:a.companyId,id:item.id,id2:d.id,checksum:d.checksum,extractionMethod:p.extractionMethod,fact6:p.inspectionId ?? null,actorId:a.userId,now:ports.clock.now()}), ...insertChunks( item.id, d.id, d.checksum, p.chunks, null, key));
  } else if (x.action === "law.applicability.propose") {
    const p = z.object({ revisionId: z.string(), applies: z.boolean(), basis: z.string().trim().min(10).max(12000) }).parse(raw);
    if (!(await ports.repository.approvedRevision({revisionId:p.revisionId,id:item.id,companyId:a.companyId}))) fail("Assess an independently approved law revision.");
    steps.push(ports.repository.requireLawRevisionCurrent(key,[p.revisionId, item.id]), ports.repository.proposeApplicability({key:key,companyId:a.companyId,id:item.id,revisionId:p.revisionId,applies:p.applies,basis:p.basis,actorId:a.userId,now:ports.clock.now()}));
  } else if (x.action === "law.applicability.approve") {
    const p = z.object({ applicabilityId: z.string(), applicabilityVersion: z.number().int().positive() }).parse(raw);
    const row = await ports.repository.applicability({applicabilityId:p.applicabilityId,id:item.id,companyId:a.companyId});
    if (!row || row.version !== p.applicabilityVersion || row.state !== "pending") fail("Applicability changed. Refresh.", "LEGAL_VERSION_CONFLICT", 409);
    if (ports.samePerson(row.created_by)) fail("An independent reviewer must verify Company applicability.", "LEGAL_INDEPENDENT_REVIEW", 403);
    evidenceId = row.id;
    steps.push(ports.repository.requireLawApplicabilityCurrent(key,[row.id, row.version, a.userId]), ports.repository.approveApplicability({actorId:a.userId,now:ports.clock.now(),id:row.id}));
  } else {
    const law = x.action.startsWith("law."), retiring = x.action === "law.revision.retire";
    const p = law ? z.object({ revisionId: z.string(), revisionVersion: z.number().int().positive() }).parse(raw) : z.object({ textReviewId: z.string(), textReviewVersion: z.number().int().positive() }).parse(raw);
    const id = "revisionId" in p ? p.revisionId : p.textReviewId, version = "revisionVersion" in p ? p.revisionVersion : p.textReviewVersion;
    const row = await ports.repository.review({id:id,id2:item.id,companyId:a.companyId,reviewKind:law?"law":"document"});
    if (!row || row.version !== version || row.state !== (retiring ? "approved" : "pending")) fail("The source review changed. Refresh.", "LEGAL_VERSION_CONFLICT", 409);
    if (!retiring) {
      if (law) z.object({ originalVerified: z.literal(true), datesVerified: z.literal(true), textVerified: z.literal(true) }).parse(raw);
      else z.object({ textVerified: z.literal(true) }).parse(raw);
      if (ports.samePerson(row.created_by)) fail("An independent reviewer must verify the original, extracted text and dates.", "LEGAL_INDEPENDENT_REVIEW", 403);
      const d = await readableSourceDocument( row.document_id, item.id);
      if (d.checksum !== row.document_checksum) fail("The source checksum changed.", "LEGAL_SOURCE_CHECKSUM", 409);
      steps.push(sourceDocumentGuard( key, d.id, item.id, d.checksum));
    }
    evidenceId = row.id;
    steps.push(ports.repository.requireLawReviewCurrent(key,law?"law":"document",retiring,[row.id, row.version, row.state, ...(!retiring ? [a.userId] : [])]), ports.repository.decideReview({fact0:retiring ? "retired" : "approved",actorId:a.userId,now:ports.clock.now(),reason:x.reason,id:row.id,reviewKind:law?"law":"document"}));
    if (law && !retiring) steps.push(ports.repository.reviewSourceCheck({id:row.id}));
  }
  steps.push(ports.repository.advanceRecord({now:ports.clock.now(),id:item.id}), ports.repository.event(item.id,x.action,x.reason,item.version + 1,ports.identifiers.next(),ports.clock.now()));
  return ports.repository.commit(key,x.action,raw,item.id,item.version + 1,{ id: item.id, evidenceId, version: item.version + 1, state: approving ? x.action.endsWith("retire") ? "retired" : "approved" : "pending" },steps);
}

async function lawCorpusView(ruleId?:string){ports.require("legal.read");if(ruleId)await ports.access(ruleId);const impacts=await ports.repository.impacts(ruleId),chunks=await ports.repository.chunks(ruleId),documents=Array.from(new Map(chunks.map(chunk=>[chunk.document_id,chunk])).values());if(documents.length)await ports.repository.recordReads(documents.map(chunk=>({identifier:ports.identifiers.next(),itemId:chunk.item_id,documentId:chunk.document_id,now:ports.clock.now()})));return {sources:await ports.repository.sources(ruleId),revisions:await ports.repository.revisions(ruleId),applicability:await ports.repository.applicabilityList(ruleId),textReviews:await ports.repository.textReviews(ruleId),checks:await ports.repository.checks(ruleId),chunks,impacts,notice:"Official-source changes and extracted text remain pending until independently reviewed. Existing deadlines require an explicit owner decision."};}

return {lawCorpusCommand,lawCorpusView};}
