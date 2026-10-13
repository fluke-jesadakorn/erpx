import type {ExecutedCopyRepository} from "../../../src/server/modules/legal/application/executed-copies";
import type {LegalDestructionRepository} from "../../../src/server/modules/legal/application/destruction-sources";
import type {LegalDocumentRepository,DocumentStorage,DocumentIntake,DocumentProvider} from "../../../src/server/modules/legal/application/documents";
type LegalReferenceRepository=Parameters<typeof import("../../../src/server/modules/legal/application/references").createLegalReferencesApplication>[0]["repository"];
type LegalProcessingRepository=Parameters<typeof import("../../../src/server/modules/legal/application/processing").createLegalProcessingApplication>[0]["repository"];
import { expect, it } from "vitest";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
import { createLegalAccessApplication, type LegalAccessRepository } from "../../../src/server/modules/legal/application/access";
import { createLegalPeopleQuery } from "../../../src/server/modules/legal/application/assignees";
import { createLegalEnrollmentAccess } from "../../../src/server/modules/legal/application/enrollment";
import { createLegalInvitationWorker, type LegalInvitationRepository } from "../../../src/server/modules/legal/application/invitations";
import { readBoundedLegalBody } from "../../../src/server/modules/legal/application/body";
import { createLegalRuntimeApplication } from "../../../src/server/modules/legal/application/runtime-gate";
import { createLegalKnowledgeApplication, type LegalKnowledgeRepository } from "../../../src/server/modules/legal/application/knowledge";
import { validateLegalAiCitations, type LegalEvidenceSource } from "../../../src/server/modules/legal/domain/knowledge";
import type { LegalItem } from "../../../src/server/modules/legal/domain/model";

function repository<T extends object>(operations: Partial<T>): T { return new Proxy(operations as T, { get(target, name) { if (name === "then") return undefined; if (name in target) return Reflect.get(target, name); return () => { throw new Error(`unexpected operation: ${String(name)}`); }; } }); }
const error = (_status: number, code: string) => Object.assign(new Error(code), {code});
const actor = { userId: "person", name: "Current person", companyId: "company", branchId: "branch", departmentId: "department", role: "legal_editor" } as WorkContext;
const item = { id: "item", kind: "contract", branch_id: "branch", department_id: "department" } as LegalItem;
const access = (source: LegalAccessRepository) => createLegalAccessApplication({ actor, repository: source, require: () => {}, error });

it("a missing visible Legal record cannot disclose its data or Company settings", async () => {
  await expect(access(repository<LegalAccessRepository>({ visible: async () => null })).access("missing")).rejects.toThrow("LEGAL_NOT_FOUND");
});
it("disabled Legal remains readable but blocks mutations before loading subtype data", async () => {
  const source = repository<LegalAccessRepository>({ visible: async () => item, enabled: async () => false, data: async () => ({ data: { terms: "retained" } }) });
  expect(await access(source).access(item.id)).toMatchObject({ data: { terms: "retained" } });
  await expect(access(source).access(item.id, "legal.edit")).rejects.toThrow("LEGAL_DISABLED");
});
it("an editor needs explicit record membership even with Company edit permission", async () => {
  const source = repository<LegalAccessRepository>({ visible: async () => item, enabled: async () => true, editor: async () => false });
  await expect(access(source).access(item.id, "legal.edit")).rejects.toThrow("LEGAL_EDITOR_REQUIRED");
});
it("a record outside the current Department cannot reveal eligible people", async () => {
  const application = createLegalPeopleQuery({ actor, require: () => {}, error, item: async () => ({ ...item, department_id: "other" }), activeUnit: async () => { throw new Error("unexpected unit read"); }, departmentInBranch: async () => false, people: async () => { throw new Error("unexpected people read"); } });
  await expect(application({ itemId: item.id })).rejects.toThrow("LEGAL_SCOPE");
});
it("reviewer selection requires approval eligibility and retains current actor label while deduplicating assignments", async () => {
  const application = createLegalPeopleQuery({ actor, require: () => {}, error, item: async () => item, activeUnit: async () => true, departmentInBranch: async () => true, people: async (_query, scope) => { expect(scope).toEqual({ branch: "branch", department: "department", permission: "legal.approve", itemId: "item" }); return [{ id: "person", name: "Stored person", role: "legal_editor" }, { id: "person", name: "Stored person", role: "legal_manager" }]; } });
  expect(await application({ purpose: "reviewer", itemId: "item" })).toEqual([{ id: "person", name: "Current person", role: "legal_manager" }]);
});
it("invalid enrollment tokens cannot resolve a Holding or consult the application clock", async () => {
  const application = createLegalEnrollmentAccess({ verify: async () => { throw new Error("invalid signature"); }, load: async () => { throw new Error("unexpected Holding"); }, clock: { now: () => { throw new Error("unexpected clock"); } }, error });
  await expect(application("tampered")).rejects.toThrow("LEGAL_INVITATION_UNAVAILABLE");
});
it("accepted or expired invitations never return the resolved Company resources", async () => {
  const application = createLegalEnrollmentAccess({ verify: async () => ({ slug: "holding", shareId: "share" }), load: async () => ({ resource: { marker: "private" }, share: async (id, now) => { expect([id, now]).toEqual(["share", 100]); return null; } }), clock: { now: () => 100 }, error });
  await expect(application("valid")).rejects.toThrow("LEGAL_INVITATION_UNAVAILABLE");
});
const job = { id: "job", share_id: "share", holding_slug: "holding", attempts: 6 };
const invitation = (source: LegalInvitationRepository, overrides: Partial<Parameters<typeof createLegalInvitationWorker>[0]> = {}) => createLegalInvitationWorker({ repository: source, clock: { now: () => 100 }, identifiers: { next: () => "lease" }, publicOrigin: "https://console.example.test", enrollmentEnabled: false, enrollmentToken: async () => { throw new Error("unexpected enrollment token"); }, send: async () => { throw new Error("unexpected delivery"); }, ...overrides });
it("a revoked or expired share cancels its claimed invitation before any delivery", async () => {
  const actions: string[] = [], source = repository<LegalInvitationRepository>({ recover: async () => {}, claim: async () => job, share: async () => null, cancel: async (id, token) => { expect([id, token]).toEqual(["job", "lease"]); actions.push("cancel"); } });
  await invitation(source)(); expect(actions).toEqual(["cancel"]);
});
it("the sixth failed invitation becomes terminal and keeps a bounded deterministic retry time", async () => {
  const source = repository<LegalInvitationRepository>({ recover: async () => {}, claim: async () => job, share: async () => ({ email: "external@example.test", expires_at: 1000 }), failed: async (id, token, state, code, nextAttempt) => { expect([id, token, state, code, nextAttempt]).toEqual(["job", "lease", "failed", "EMAIL_DELIVERY_FAILED", 100 + 30000 * 64]); } });
  await invitation(source)();
});
it("a missing public origin preserves its explicit prerequisite failure without calling a provider", async () => {
  const source = repository<LegalInvitationRepository>({ recover: async () => {}, claim: async () => ({ ...job, attempts: 1 }), share: async () => ({ email: "external@example.test", expires_at: 1000 }), failed: async (_id, _token, state, code) => { expect([state, code]).toEqual(["queued", "PUBLIC_ORIGIN_NOT_CONFIGURED"]); } });
  await invitation(source, { publicOrigin: undefined })();
});
it("a declared oversized body is cancelled before acquiring a stream reader", async () => {
  let cancelled = false;
  await expect(readBoundedLegalBody({ declaredLength: "2049", cancel: async () => { cancelled = true; }, reader: () => { throw new Error("unexpected reader"); } }, 2048, error)).rejects.toThrow("BODY_TOO_LARGE");
  expect(cancelled).toBe(true);
});
it("an oversized streamed body cancels and releases the reader even without a declared size", async () => {
  const order: string[] = [];
  await expect(readBoundedLegalBody({ declaredLength: null, cancel: async () => {}, reader: () => ({ read: async () => ({ done: false, value: new Uint8Array(3) }), cancel: async () => { order.push("cancel"); }, releaseLock: () => { order.push("release"); } }) }, 2, error)).rejects.toThrow("BODY_TOO_LARGE");
  expect(order).toEqual(["cancel", "release"]);
});
it("a runtime heartbeat becomes stale strictly after five minutes", async () => {
  const application = createLegalRuntimeApplication({ require: () => {}, clock: { now: () => 300001 }, ready: async () => false, tasks: async () => [{ task: "documents", heartbeat_at: 1, last_success_at: null, failure_count: 0, last_error: null }, { task: "invitations", heartbeat_at: 0, last_success_at: null, failure_count: 1, last_error: "prior failure" }] });
  expect(await application.legalRuntimeHealth()).toMatchObject({ companyReady: false, tasks: [{ stale: false }, { stale: true }] });
});
const source: LegalEvidenceSource = { id: "chunk", sourceId: "law:chunk", kind: "law", item_id: "rule", title: "Official original", document_id: "document", version: 1, checksum: "digest", text: "An exact retained quotation supports review.", excerpt: "An exact retained quotation supports review.", page: 1, section: "Section 1" };
it("overlapping approved law revisions keep AI evidence insufficient despite accessible documents", async () => {
  const application = createLegalKnowledgeApplication({ require: () => {}, access: async () => item, document: async () => { throw new Error("unexpected document"); }, clock: { now: () => 100 }, identifiers: { next: () => "event" }, error, repository: repository<LegalKnowledgeRepository>({ reviewed: async () => [], authored: async () => [], lawSources: async () => [source], lawConflicts: async () => 1, authoredItem: async () => [], recordReads: async rows => { expect(rows).toHaveLength(1); } }) });
  expect(await application.retrieveLegalAiEvidence(item.id, "quotation")).toMatchObject({ conflicts: 1, insufficientEvidence: true, sources: [source] });
});
it("citation presence cannot replace an exact supplied quotation or independent legal review", () => {
  const evidence = { sources: [source], asOf: "2026-10-06", conflicts: 0, insufficientEvidence: false, notice: "review required" };
  expect(() => validateLegalAiCitations({ sources: [source.sourceId], claims: [{ text: "Conclusion", sources: [source.sourceId], quotes: [{ sourceId: source.sourceId, quote: "invented quotation" }] }] }, evidence, error)).toThrow("LEGAL_AI_CITATION");
  expect(validateLegalAiCitations({ sources: [source.sourceId], claims: [{ text: "Conclusion", sources: [source.sourceId], quotes: [{ sourceId: source.sourceId, quote: "An exact retained quotation" }] }] }, evidence, error)).toMatchObject({ evidenceStatus: "pending_legal_review", claims: [{ supportStatus: "quotation_verified_pending_legal_review", requiresHumanReview: true }] });
});

it("a Legal command replay returns retained results before stale-version checks or staging writes", async () => {
  const { createLegalRegisterApplication } = await import("../../../src/server/modules/legal/application/service");
  const source = repository<import("../../../src/server/modules/legal/application/service").LegalRegisterRepository>({ replay: async () => ({ id: "item", version: 2 }) as never });
  const application = createLegalRegisterApplication({ actor, repository: source, require: () => {}, can: () => true, samePerson: () => false, permissionCatalog: [], clock: { now: () => { throw new Error("unexpected clock"); } }, identifiers: { next: () => { throw new Error("unexpected identifier"); } }, error, hash: () => "digest", utf8Length: () => 0, access: async () => ({ ...item, version: 99, state: "active" }), eligible: async () => {}, people: async () => [] });
  expect(await application.legalCommand({ action: "comment", id: "item", expectedVersion: 1 }, "key")).toEqual({ id: "item", version: 2 });
});
it("a requester cannot change a submitted question despite current edit permission", async () => {
  const { createLegalRegisterApplication } = await import("../../../src/server/modules/legal/application/service");
  const application = createLegalRegisterApplication({ actor: { ...actor, role: "legal_requester" }, repository: repository<import("../../../src/server/modules/legal/application/service").LegalRegisterRepository>({ replay: async () => null }), require: () => {}, can: () => true, samePerson: () => false, permissionCatalog: [], clock: { now: () => { throw new Error("unexpected clock"); } }, identifiers: { next: () => "identifier" }, error, hash: () => "digest", utf8Length: () => 0, access: async () => ({ ...item, kind: "request", version: 1, state: "review" }), eligible: async () => {}, people: async () => [] });
  await expect(application.legalCommand({ action: "update", id: "item", expectedVersion: 1 }, "key")).rejects.toThrow("LEGAL_REQUESTER_SCOPE");
});
it("the latest Legal content author cannot independently approve another creator's record", async () => {
  const { createLegalRegisterApplication } = await import("../../../src/server/modules/legal/application/service");
  const application = createLegalRegisterApplication({ actor, repository: repository<import("../../../src/server/modules/legal/application/service").LegalRegisterRepository>({ replay: async () => null, lastAuthor: async () => ({ actor_id: "person" }) }), require: () => {}, can: () => true, samePerson: id => id === "person", permissionCatalog: [], clock: { now: () => { throw new Error("unexpected clock"); } }, identifiers: { next: () => "identifier" }, error, hash: () => "digest", utf8Length: () => 0, access: async () => ({ ...item, version: 1, state: "approval", created_by: "other" }), eligible: async () => {}, people: async () => [] });
  await expect(application.legalCommand({ action: "approve", id: "item", expectedVersion: 1 }, "key")).rejects.toThrow("LEGAL_INDEPENDENT_REVIEW");
});

it("a trust manager cannot approve their own proposed CA material", async () => {
  const { createLegalTrustApplication } = await import("../../../src/server/modules/legal/application/trust");
  const application = createLegalTrustApplication({ companyScope: true, require: () => {}, samePerson: id => id === "person", certificate: () => { throw new Error("unexpected certificate"); }, decodeBase64: () => new Uint8Array(), encodeBase64: () => "encoded", hash: () => "digest", clock: { now: () => 100 }, error, repository: repository<import("../../../src/server/modules/legal/application/trust").LegalTrustRepository>({ replay: async () => null, lock: () => ({ owner: "legal" }), material: async () => ({ id: "material", state: "proposed", version: 1, created_by: "person" }) }) });
  await expect(application.trustCommand({ action: "trust.approve", materialId: "material", materialVersion: 1 }, "key")).rejects.toThrow("LEGAL_INDEPENDENT_REVIEW");
});
it("an expired or non-CA certificate cannot stage proposed trust material", async () => {
  const { createLegalTrustApplication } = await import("../../../src/server/modules/legal/application/trust");
  for (const certificate of [{ ca: false, validTo: "2027-01-01", raw: new Uint8Array() }, { ca: true, validTo: "2000-01-01", raw: new Uint8Array() }]) {
    const application = createLegalTrustApplication({ companyScope: true, require: () => {}, samePerson: () => false, certificate: () => certificate, decodeBase64: () => new Uint8Array(), encodeBase64: () => "encoded", hash: () => "digest", clock: { now: () => 1800000000000 }, error, repository: repository<import("../../../src/server/modules/legal/application/trust").LegalTrustRepository>({ replay: async () => null, lock: () => ({ owner: "legal" }) }) });
    await expect(application.trustCommand({ action: "trust.add", kind: "root", name: "Root CA", source: "https://official.example.test", content: "Declared fake certificate input" }, "key")).rejects.toThrow("LEGAL_INVALID");
  }
});
it("a malformed validator status remains indeterminate and never claims a valid PDF signature", async () => {
  const { createSignedPdfValidation } = await import("../../../src/server/modules/legal/application/pdf-validation");
  const application = createSignedPdfValidation({ materials: async () => [{ id: "material", kind: "root", content: "private certificate body", checksum: "digest" }], prepare: async () => {}, validate: async () => ({ status: ["valid"] }) });
  expect(await application()).toEqual({ status: "indeterminate", reasons: ["VALIDATOR_UNAVAILABLE_OR_TIMED_OUT"], signatures: [], trustMaterials: [{ id: "material", kind: "root", checksum: "digest" }] });
});
it("PDF trust preparation failures propagate before the provider fallback and do not leak trust bodies", async () => {
  const { createSignedPdfValidation } = await import("../../../src/server/modules/legal/application/pdf-validation");
  const application = createSignedPdfValidation({ materials: async () => [], prepare: async () => { throw new Error("trust preparation failed"); }, validate: async () => { throw new Error("unexpected validator"); } });
  await expect(application()).rejects.toThrow("trust preparation failed");
});

it("an executed-copy replay still rechecks current eligibility and explicit reviewer membership",async()=>{
 const {createExecutedCopyApplication}=await import("../../../src/server/modules/legal/application/executed-copies");
 const actions:string[]=[];const application=createExecutedCopyApplication({access:async()=>({...item,version:99}),eligible:async()=>{actions.push("eligible");},clock:{now:()=>{throw new Error("unexpected clock");}},identifiers:{next:()=>"unused"},error,repository:repository<ExecutedCopyRepository>({member:async()=>{actions.push("member");return true;},replay:async()=>({retained:true})})});
 expect(await application.reviewExecutedCopy({action:"executed.review",id:"item",expectedVersion:1,executedCopyId:"copy",decision:"confirm",checks:{contentMatches:true,pagesComplete:true,partiesVerified:true,signaturesComplete:true,authorityVerified:true},reason:"Independent review completed"},"key")).toEqual({retained:true});expect(actions).toEqual(["eligible","member"]);
});
it("references conceal inaccessible target existence and labels while keeping accessible order",async()=>{
 const {createLegalReferencesApplication}=await import("../../../src/server/modules/legal/application/references");
 const application=createLegalReferencesApplication({access:async()=>item,requirePeople:()=>{throw new Error("denied");},target:async(kind,id)=>{if(id==="private")throw new Error("private");return{order_number:"Visible order",version:2};},repository:repository<LegalReferenceRepository>({list:async()=>[{target_module:"person",target_id:"person"},{target_module:"commerce_order",target_id:"private"},{target_module:"commerce_order",target_id:"visible"}]}),clock:{now:()=>0},identifiers:{next:()=>"event"}});
 expect(await application.legalReferences("item")).toEqual([{kind:"commerce_order",id:"visible",label:"Visible order",version:2,path:"commerce/orders/visible"}]);
});
it("a processing worker refuses a withdrawn assignment before refreshing authority or reading document bytes",async()=>{
 const {createLegalProcessingApplication}=await import("../../../src/server/modules/legal/application/processing");
 let failure="";const application=createLegalProcessingApplication({actor:()=>actor,clock:{now:()=>100},identifiers:{next:()=>"lease"},error,access:async()=>{throw new Error("unexpected access");},documents:{access:async()=>{throw new Error("unexpected document");},inspect:async()=>{throw new Error("unexpected provider");}},companyReady:async()=>true,assignmentCurrent:async()=>false,refreshAuthority:async()=>{throw new Error("unexpected authority");},repository:repository<LegalProcessingRepository>({recover:async()=>{},claim:async()=>({id:"job",item_id:"item",document_id:"document",kind:"ocr",checksum:"digest",actor_json:JSON.stringify(actor)}),failed:async(_id,_token,code)=>{failure=code;}})});
 await application.drainLegalProcessing();expect(failure).toBe("LEGAL_INVALID");
});
it("destruction refuses a shared canonical source before any ledger retirement",async()=>{
 const {createLegalDestructionApplication}=await import("../../../src/server/modules/legal/application/destruction-sources");
 const application=createLegalDestructionApplication({clock:{now:()=>0},repository:repository<LegalDestructionRepository>({lockedSource:async()=>({object_key:"original",checksum_sha256:"digest",source_state:"available",legal_hold:0}),linkCurrent:async()=>({}),sourceShared:async()=>({}),})});
 await expect(application.validateLegalDestructionSources("item","company",{documents:[],uploads:[],keys:["original"],canonicalSources:[{id:"source",objectKey:"original",checksum:"digest",linkIds:["link"],ledger:[]}]})).rejects.toThrow("LEGAL_DESTRUCTION_SOURCE_SHARED");
});
it("privacy validation compares the known time against its injected clock",async()=>{
 const {validatePrivacyCase}=await import("../../../src/lib/legal/legal/features-domain");
 const data={caseType:"breach",knownAt:"2026-10-06T03:00:00Z"};
 expect(()=>validatePrivacyCase(data,Date.parse("2026-10-06T02:59:59Z"))).toThrow("future");expect(()=>validatePrivacyCase(data,Date.parse(data.knownAt))).not.toThrow();
});

async function documentApplication(overrides:Partial<Parameters<typeof import("../../../src/server/modules/legal/application/documents").createLegalDocumentApplication>[0]>={}){
 const {createLegalDocumentApplication}=await import("../../../src/server/modules/legal/application/documents");
 return createLegalDocumentApplication({actor,repository:repository<LegalDocumentRepository>({}),storage:repository<DocumentStorage>({}),fileIntake:repository<DocumentIntake>({}),provider:repository<DocumentProvider>({}),access:async()=>({...item,state:"draft",version:1}),clock:{now:()=>100},identifiers:{next:()=>"inspection"},hash:()=>"digest",utf8:value=>new TextEncoder().encode(value),ascii:bytes=>new TextDecoder().decode(bytes),hex:bytes=>Array.from(bytes).map(value=>value.toString(16).padStart(2,"0")).join(""),error,...overrides});
}
it("a Legal upload rejects mismatched file signatures before reserving or replaying evidence",async()=>{
 const application=await documentApplication();
 await expect(application.uploadDocument("item",1,"scan.pdf","application/pdf",new TextEncoder().encode("text"),"key")).rejects.toThrow("LEGAL_INVALID");
});
it("malware rejection blocks document binding and owner commit after the scanner result",async()=>{
 const application=await documentApplication({repository:repository<LegalDocumentRepository>({replay:async()=>null}),fileIntake:repository<DocumentIntake>({readOnly:()=>false,createSet:async()=>({id:"set"}),reserve:async()=>({uploadIntentId:"intent"}),complete:async()=>({sourceId:"source",attachmentLinkId:"link"}),scan:async()=>"malware_rejected"})});
 await expect(application.uploadDocument("item",1,"scan.pdf","application/pdf",new TextEncoder().encode("%PDF-"),"key")).rejects.toThrow("LEGAL_UPLOAD_REJECTED");
});
it("a stored source checksum mismatch blocks Legal document binding and all staged writes",async()=>{
 const application=await documentApplication({repository:repository<LegalDocumentRepository>({replay:async()=>null,cleanSource:async()=>({object_key:"original",checksum_sha256:"different",media_type:"application/pdf",bytes:5})}),fileIntake:repository<DocumentIntake>({readOnly:()=>false,createSet:async()=>({id:"set"}),reserve:async()=>({uploadIntentId:"intent"}),complete:async()=>({sourceId:"source",attachmentLinkId:"link"}),scan:async()=>"available"})});
 await expect(application.uploadDocument("item",1,"scan.pdf","application/pdf",new TextEncoder().encode("%PDF-"),"key")).rejects.toThrow("LEGAL_INTEGRITY");
});
it("an unmigrated stored document cannot bypass the canonical attachment authorization port",async()=>{
 const application=await documentApplication({repository:repository<LegalDocumentRepository>({document:async()=>({id:"document",item_id:"item",object_key:"old-object",attachment_link_id:null}) as import("../../../src/server/modules/legal/domain/model").DocumentVersion,disposed:async()=>null,destroying:async()=>null})});
 await expect(application.documentExport("document",null)).rejects.toThrow("LEGAL_ATTACHMENT_LINK_REQUIRED");
});
it("the inspection provider session is cleaned after a current preservation guard refuses persistence",async()=>{
 const actions:string[]=[];const document={id:"document",item_id:"item",object_key:null,body:"retained",checksum:"digest",media_type:"text/plain"} as import("../../../src/server/modules/legal/domain/model").DocumentVersion;
 const application=await documentApplication({repository:repository<LegalDocumentRepository>({document:async()=>document,disposed:async()=>null,destroying:async()=>null,preservationLock:()=>({owner:"legal"}),insertInspection:()=>({owner:"legal"}),persistInspection:async()=>0}),provider:repository<DocumentProvider>({prepare:async()=>({write:async()=>{},verify:async()=>{throw new Error("unexpected verification");},extract:async()=>({text:"retained",pages:[],pageMapping:true}),close:async()=>{actions.push("close");}})})});
 await expect(application.inspectDocument("document","ocr")).rejects.toThrow("LEGAL_DOCUMENT_FROZEN");expect(actions).toEqual(["close"]);
});
it("document OCR evidence clips the total retained text and pages while preserving inspection identity",async()=>{
 const text="x".repeat(200001),document={id:"document",item_id:"item",object_key:null,body:"retained",checksum:"digest",media_type:"text/plain"} as import("../../../src/server/modules/legal/domain/model").DocumentVersion;
 const application=await documentApplication({repository:repository<LegalDocumentRepository>({document:async()=>document,disposed:async()=>null,destroying:async()=>null,preservationLock:()=>({owner:"legal"}),insertInspection:()=>({owner:"legal"}),persistInspection:async()=>1,recordInspectionRead:async()=>{}}),provider:repository<DocumentProvider>({prepare:async()=>({write:async()=>{},verify:async()=>({}),extract:async()=>({text,pages:[{page:1,text:text.slice(0,150000)},{page:2,text:text.slice(0,100000)}],pageMapping:true}),close:async()=>{}})})});
 const result=await application.inspectDocument("document","ocr");expect(result).toMatchObject({inspectionId:"inspection",pageMapping:false,truncated:true,requiresReview:true,checksum:"digest"});expect(String(result.text)).toHaveLength(200000);expect((result.pages as {text:string}[]).map(page=>page.text.length)).toEqual([150000,50000]);
});

type PortalPorts=Parameters<typeof import("../../../src/server/modules/legal/application/portal").createLegalPortalApplication>[0];
type PortalAttachmentPorts=Parameters<typeof import("../../../src/server/modules/legal/application/portal-attachments").createPortalAttachmentApplication>[0];
type OperationsPorts=Parameters<typeof import("../../../src/server/modules/legal/application/operations").createLegalOperationsApplication>[0];
const share={id:"share",item_id:"item",document_id:"document",company_id:"company",branch_id:null,department_id:null,item_version:2,kind:"contract",version:1};
const portalKey="8fa507ae-c8f2-48a7-b056-5c4efc7085bd";
async function portalApplication(overrides:Partial<PortalPorts>={}){
 const {createLegalPortalApplication}=await import("../../../src/server/modules/legal/application/portal");
 return createLegalPortalApplication({actor:{userId:"outside",email:"outside@example.test",sessionId:"session"},repository:repository<PortalPorts["repository"]>({}),clock:{now:()=>100},identifiers:{next:()=>"event"},hash:()=>"digest",decode:bytes=>new TextDecoder().decode(bytes),error,attachments:repository<PortalPorts["attachments"]>({}),...overrides});
}
async function portalAttachmentApplication(overrides:Partial<PortalAttachmentPorts>={}){
 const {createPortalAttachmentApplication}=await import("../../../src/server/modules/legal/application/portal-attachments");
 return createPortalAttachmentApplication({repository:repository<PortalAttachmentPorts["repository"]>({}),storage:repository<PortalAttachmentPorts["storage"]>({}),clock:{now:()=>100},identifiers:{next:()=>"identifier"},hash:()=>"digest",reservationMs:300000,readOnly:()=>false,uploadsFrozen:()=>false,readiness:async()=>[{enabled:true,ready:true},{ready:true}],policy:()=>({version:1,classification:"restricted",retentionPolicyKey:"legal-managed",requiredCount:0,maxFiles:10}),assertMedia:()=>{},detectMedia:()=>"application/pdf",scan:async()=>{throw new Error("unexpected scanner");},error,errorCode:value=>value instanceof Error&&"code"in value?String(value.code):null,...overrides});
}
async function operationsApplication(overrides:Partial<OperationsPorts>={}){
 const {createLegalOperationsApplication}=await import("../../../src/server/modules/legal/application/operations");
 return createLegalOperationsApplication({actor,repository:repository<OperationsPorts["repository"]>({}),clock:{now:()=>100},identifiers:{next:()=>"identifier"},hash:()=>"digest",byteLength:text=>new TextEncoder().encode(text).byteLength,require:()=>{},error,...overrides});
}
it("a revoked portal share conceals its existence before retention or document reads",async()=>{
 const application=await portalApplication({repository:repository<PortalPorts["repository"]>({share:async()=>null})});
 await expect(application.portalDetail("share")).rejects.toThrow("LEGAL_SHARE_UNAVAILABLE");
});
it("portal retention prevents a previously shared original from reaching the file port",async()=>{
 const application=await portalApplication({repository:repository<PortalPorts["repository"]>({share:async()=>share,retentionFrozen:async()=>({})})});
 await expect(application.portalDownload("share")).rejects.toThrow("LEGAL_RETENTION_FROZEN");
});
it("portal download rechecks a revoked share after reading the canonical bytes and before recording success",async()=>{
 let reads=0;const application=await portalApplication({repository:repository<PortalPorts["repository"]>({share:async()=>++reads===1?share:null,retentionFrozen:async()=>null,document:async()=>({id:"document",name:"Original.pdf",attachment_link_id:"link"}) as import("../../../src/server/modules/legal/domain/model").DocumentVersion}),attachments:repository<PortalPorts["attachments"]>({read:async()=>({bytes:new Uint8Array([1]),filename:"Original.pdf",mediaType:"application/pdf"})})});
 await expect(application.portalDownload("share")).rejects.toThrow("LEGAL_SHARE_UNAVAILABLE");expect(reads).toBe(2);
});
it("new portal signing is disabled before a transaction or MFA provider can be reached",async()=>{
 const application=await portalApplication();
 for(const action of ["sign","decline"])await expect(application.portalCommand({shareId:"share",action},portalKey)).rejects.toThrow("LEGAL_SIGNING_DISABLED");
});
it("a portal receipt for another actor cannot replay despite an otherwise current share",async()=>{
 const unit=repository<import("../../../src/server/modules/legal/application/portal").LegalPortalUnit>({share:async()=>share,retentionFrozen:async()=>null,lock:async()=>{},receipt:async()=>({user_id:"other",request_hash:"digest",result_json:'{"id":"share"}'})});
 const application=await portalApplication({repository:repository<PortalPorts["repository"]>({transaction:async work=>work(unit)})});
 await expect(application.portalCommand({shareId:"share",action:"accept"},portalKey)).rejects.toThrow("IDEMPOTENCY_CONFLICT");
});
it("a Legal item changed during portal scanning cannot bind the clean source or create a receipt",async()=>{
 const unit=repository<import("../../../src/server/modules/legal/application/portal").LegalPortalUnit>({lock:async()=>{},share:async()=>({...share,item_version:3}),retentionFrozen:async()=>null,uploadEnabled:async()=>({}),uploadReplay:async()=>null});
 const application=await portalApplication({repository:repository<PortalPorts["repository"]>({share:async()=>share,retentionFrozen:async()=>null,uploadReceipt:async()=>null,transaction:async work=>work(unit)}),attachments:repository<PortalPorts["attachments"]>({stage:async()=>({setId:"set",linkId:"link",sourceId:"source",objectKey:"original",checksum:"digest",mediaType:"application/pdf",bytes:5,expectedTargetVersion:2})})});
 await expect(application.portalUpload("share","Original.pdf","application/pdf",new TextEncoder().encode("%PDF-"),portalKey)).rejects.toThrow("LEGAL_VERSION_CONFLICT");
});
it("portal file intake refuses unavailable scanner prerequisites before reading quotas or storing bytes",async()=>{
 const application=await portalAttachmentApplication({readiness:async()=>[{enabled:true,ready:false},{ready:true}]});
 await expect(application.stagePortalAttachment("outside",{companyId:"company",branchId:null,departmentId:null,itemId:"item",itemVersion:1,shareId:"share"},"original.pdf","application/pdf",new Uint8Array([1]),portalKey)).rejects.toThrow("ATTACHMENT_SCANNER_UNAVAILABLE");
});
it("an existing portal upload key cannot be reused for different file content",async()=>{
 const application=await portalAttachmentApplication({repository:repository<PortalAttachmentPorts["repository"]>({existing:async()=>({object_key:"original",checksum_sha256:"different",media_type:"application/pdf",bytes:1})})});
 await expect(application.stagePortalAttachment("outside",{companyId:"company",branchId:null,departmentId:null,itemId:"item",itemVersion:1,shareId:"share"},"original.pdf","application/pdf",new Uint8Array([1]),portalKey)).rejects.toThrow("IDEMPOTENCY_CONFLICT");
});
it("reserved and pending portal bytes both count toward Company storage before object writes",async()=>{
 const application=await portalAttachmentApplication({repository:repository<PortalAttachmentPorts["repository"]>({existing:async()=>null,usedStorage:async()=>8,pendingStorage:async()=>2,storageAllowance:async()=>({storage_bytes:10})})});
 await expect(application.stagePortalAttachment("outside",{companyId:"company",branchId:null,departmentId:null,itemId:"item",itemVersion:1,shareId:"share"},"original.pdf","application/pdf",new Uint8Array([1]),portalKey)).rejects.toThrow("STORAGE_QUOTA");
});
it("a canonical portal source checksum failure records denial and never records an allowed read",async()=>{
 const codes:string[]=[];const application=await portalAttachmentApplication({hash:()=>"different",repository:repository<PortalAttachmentPorts["repository"]>({readableSource:async()=>({link_id:"link",attachment_id:"source",object_key:"original",media_type:"application/pdf",original_name:"original.pdf",checksum_sha256:"digest",source_state:"available"}),deniedRead:async facts=>{codes.push(facts.fact5);return {results:[]};}}),storage:repository<PortalAttachmentPorts["storage"]>({get:async()=>({arrayBuffer:async()=>new ArrayBuffer(1)})})});
 await expect(application.portalAttachmentBytes({companyId:"company",itemId:"item",linkId:"link",actorId:"outside",purposeKey:"legal_document"})).rejects.toThrow("LEGAL_INTEGRITY");expect(codes).toEqual(["LEGAL_INTEGRITY"]);
});
it("a changed Legal import digest cannot open its unit of work",async()=>{
 const application=await operationsApplication();
 await expect(application.importCommit({rows:[{kind:"contract",title:"Isolated agreement",data:{contractType:"Supply",parties:"Company and Supplier"}}],digest:"changed"},portalKey)).rejects.toThrow("LEGAL_INVALID");
});
it("a counsel purchase replay requires current Legal edit access before returning the retained result",async()=>{
 const steps:string[]=[];const retained={id:"item",version:2,requestId:"request"};const unit=repository<import("../../../src/server/modules/legal/application/operations").LegalOperationUnit>({access:async(id,permission)=>{steps.push(String(permission));return {...item,version:99};},replay:async<Result extends object=Record<string,unknown>>():Promise<Result|null>=>retained as Result});
 const application=await operationsApplication({repository:repository<OperationsPorts["repository"]>({transaction:async work=>work(unit)})});
 expect(await application.prepareCounselPurchase({id:"item",expectedVersion:1,summary:"Fictional counsel",estimatedCost:"10.00"},portalKey)).toEqual(retained);expect(steps).toEqual(["legal.edit"]);
});
